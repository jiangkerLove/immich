use std::path::{Path, PathBuf};
use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::asset_delete;
use crate::models::db::assets::{self, NewLibraryAsset};
use crate::models::db::library::{self, LibraryAssetSyncRow, LibraryRow};
use crate::service::job::JobService;
use crate::utils::checksum::sha1_bytes;
use crate::utils::file_walk::walk_file_batches;
use crate::utils::glob::path_matches_exclusion;
use crate::utils::mime_types::{is_video_path, supported_file_extensions};
use crate::utils::storage::StoragePaths;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_LIBRARY: &str = "library";
const JOBS_LIBRARY_PAGINATION_SIZE: usize = 10_000;

#[derive(Clone)]
pub struct LibraryProcessor {
    pool: PgPool,
    storage: StoragePaths,
    jobs: JobService,
}

#[derive(Debug, Deserialize)]
struct LibraryIdJob {
    id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibrarySyncFilesJob {
    library_id: Uuid,
    paths: Vec<String>,
    #[serde(default)]
    progress_counter: Option<u64>,
    #[serde(default)]
    total_assets: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibrarySyncAssetsJob {
    library_id: Uuid,
    import_paths: Vec<String>,
    exclusion_patterns: Vec<String>,
    asset_ids: Vec<Uuid>,
    #[serde(default)]
    progress_counter: Option<u64>,
    #[serde(default)]
    total_assets: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryRemoveAssetJob {
    library_id: Uuid,
    paths: Vec<String>,
}

impl LibraryProcessor {
    pub fn new(pool: PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self {
            pool,
            storage,
            jobs,
        }
    }

    pub async fn process(&self, name: &str, data: &Value) -> Result<(), String> {
        match name {
            "LibraryScanQueueAll" => self.handle_scan_queue_all().await,
            "LibraryDeleteCheck" => self.handle_delete_check().await,
            "LibraryDelete" => {
                let job: LibraryIdJob =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                self.handle_delete(job.id).await
            }
            "LibrarySyncFilesQueueAll" => {
                let job: LibraryIdJob =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                self.handle_sync_files_queue_all(job.id).await
            }
            "LibrarySyncFiles" => {
                let job: LibrarySyncFilesJob =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                self.handle_sync_files(job).await
            }
            "LibrarySyncAssetsQueueAll" | "LibraryScanAssetsQueueAll" => {
                let job: LibraryIdJob =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                self.handle_sync_assets_queue_all(job.id).await
            }
            "LibrarySyncAssets" => {
                let job: LibrarySyncAssetsJob =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                self.handle_sync_assets(job).await
            }
            "LibraryRemoveAsset" => {
                let job: LibraryRemoveAssetJob =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                self.handle_remove_asset(job).await
            }
            other => {
                eprintln!("library job {other} is not implemented in rust-server yet; skipping");
                Ok(())
            }
        }
    }

    async fn handle_scan_queue_all(&self) -> Result<(), String> {
        println!("Initiating scan of all external libraries...");

        self.jobs
            .queue_json_job_empty(QUEUE_LIBRARY, "LibraryDeleteCheck")
            .await
            .map_err(|err| err.to_string())?;

        let libraries = library::list_all_with_deleted(&self.pool)
            .await
            .map_err(|err| err.to_string())?;

        for library_row in &libraries {
            if library_row.deleted_at.is_some() {
                continue;
            }
            let data = serde_json::json!({ "id": library_row.id });
            self.jobs
                .queue_json_job(QUEUE_LIBRARY, "LibrarySyncFilesQueueAll", data.clone())
                .await
                .map_err(|err| err.to_string())?;
            self.jobs
                .queue_json_job(QUEUE_LIBRARY, "LibraryScanAssetsQueueAll", data)
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(())
    }

    async fn handle_delete_check(&self) -> Result<(), String> {
        let pending = library::list_deleted(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if pending.is_empty() {
            return Ok(());
        }

        println!(
            "Found {} libraries pending deletion, cleaning up...",
            pending.len()
        );
        for library_row in pending {
            self.jobs
                .queue_json_job(
                    QUEUE_LIBRARY,
                    "LibraryDelete",
                    serde_json::json!({ "id": library_row.id }),
                )
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    async fn handle_delete(&self, library_id: Uuid) -> Result<(), String> {
        asset_delete::mark_deleted_by_library(&self.pool, &library_id)
            .await
            .map_err(|err| err.to_string())?;

        let asset_ids = asset_delete::list_ids_by_library(&self.pool, &library_id)
            .await
            .map_err(|err| err.to_string())?;

        if asset_ids.is_empty() {
            println!("Deleting library {library_id}");
            library::hard_delete(&self.pool, &library_id)
                .await
                .map_err(|err| err.to_string())?;
            return Ok(());
        }

        for chunk in asset_ids.chunks(JOBS_LIBRARY_PAGINATION_SIZE) {
            for asset_id in chunk {
                self.jobs
                    .queue_json_job(
                        "backgroundTask",
                        "AssetDelete",
                        serde_json::json!({ "id": asset_id, "deleteOnDisk": false }),
                    )
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        Ok(())
    }

    async fn handle_sync_files_queue_all(&self, library_id: Uuid) -> Result<(), String> {
        let Some(library_row) = library::get_by_id(&self.pool, &library_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            println!("Library {library_id} not found, skipping refresh");
            return Ok(());
        };

        let valid_paths = self.validate_import_paths(&library_row).await?;
        if valid_paths.is_empty() {
            println!("No valid import paths found for library {library_id}");
            return Ok(());
        }

        let extensions = supported_file_extensions();
        let roots: Vec<PathBuf> = valid_paths.iter().map(PathBuf::from).collect();
        let batches = walk_file_batches(&roots, Some(&extensions), JOBS_LIBRARY_PAGINATION_SIZE);

        let mut crawl_count = 0usize;
        let mut import_count = 0usize;

        println!(
            "Starting disk crawl of {} import path(s) for library {library_id}...",
            valid_paths.len()
        );

        for path_batch in batches {
            let filtered: Vec<String> = path_batch
                .into_iter()
                .filter(|path| !is_hidden_path(path))
                .filter(|path| !path_matches_exclusion(path, &library_row.exclusion_patterns))
                .collect();

            crawl_count += filtered.len();
            let new_paths = assets::filter_new_external_paths(&self.pool, &library_id, &filtered)
                .await
                .map_err(|err| err.to_string())?;

            if !new_paths.is_empty() {
                import_count += new_paths.len();
                self.jobs
                    .queue_json_job(
                        QUEUE_LIBRARY,
                        "LibrarySyncFiles",
                        serde_json::json!({
                            "libraryId": library_id,
                            "paths": new_paths,
                            "progressCounter": crawl_count,
                        }),
                    )
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        println!(
            "Finished disk crawl for library {library_id}: crawled {crawl_count} file(s), queued {import_count} for import"
        );

        library::update_refreshed_at(&self.pool, &library_id)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn handle_sync_files(&self, job: LibrarySyncFilesJob) -> Result<(), String> {
        let Some(library_row) = library::get_by_id(&self.pool, &job.library_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            println!("Library {} not found, skipping file import", job.library_id);
            return Ok(());
        };

        let mut created_ids = Vec::new();
        for path in &job.paths {
            match self
                .import_external_file(path, &library_row.owner_id, &job.library_id)
                .await
            {
                Ok(Some(asset_id)) => created_ids.push(asset_id),
                Ok(None) => {}
                Err(err) => eprintln!(
                    "Error processing {path} for library {}: {err}",
                    job.library_id
                ),
            }
        }

        if !created_ids.is_empty() {
            println!(
                "Imported {} file(s) into library {}",
                created_ids.len(),
                job.library_id
            );
            for asset_id in &created_ids {
                let _ = crate::service::workflow_trigger::on_asset_trigger(
                    &self.pool,
                    &self.jobs,
                    &library_row.owner_id,
                    asset_id,
                    crate::utils::workflow::TRIGGER_ASSET_CREATE,
                )
                .await;
            }
            self.queue_post_sync_jobs(&created_ids).await?;
        }

        Ok(())
    }

    async fn handle_sync_assets_queue_all(&self, library_id: Uuid) -> Result<(), String> {
        let Some(library_row) = library::get_by_id(&self.pool, &library_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(());
        };

        let has_assets = asset_delete::library_has_assets(&self.pool, &library_id)
            .await
            .map_err(|err| err.to_string())?;
        if !has_assets {
            println!("Library {library_id} is empty, no need to check assets");
            return Ok(());
        }

        let affected = library::detect_offline_external_assets(
            &self.pool,
            &library_id,
            &library_row.import_paths,
            &library_row.exclusion_patterns,
        )
        .await
        .map_err(|err| err.to_string())?;
        println!(
            "{affected} asset(s) were offlined due to import paths and/or exclusion pattern(s) in library {library_id}"
        );

        let asset_ids = asset_delete::list_ids_by_library(&self.pool, &library_id)
            .await
            .map_err(|err| err.to_string())?;
        let total_assets = asset_ids.len();

        let mut count = 0usize;
        for chunk in asset_ids.chunks(JOBS_LIBRARY_PAGINATION_SIZE) {
            count += chunk.len();
            self.jobs
                .queue_json_job(
                    QUEUE_LIBRARY,
                    "LibrarySyncAssets",
                    serde_json::json!({
                        "libraryId": library_id,
                        "importPaths": library_row.import_paths,
                        "exclusionPatterns": library_row.exclusion_patterns,
                        "assetIds": chunk,
                        "progressCounter": count,
                        "totalAssets": total_assets,
                    }),
                )
                .await
                .map_err(|err| err.to_string())?;
        }

        println!("Finished queuing {count} asset check(s) for library {library_id}");
        Ok(())
    }

    async fn handle_sync_assets(&self, job: LibrarySyncAssetsJob) -> Result<(), String> {
        let assets = library::list_assets_for_sync(&self.pool, &job.asset_ids)
            .await
            .map_err(|err| err.to_string())?;

        let mut active_offline = Vec::new();
        let mut trashed_offline = Vec::new();
        let mut active_online = Vec::new();
        let mut trashed_online = Vec::new();
        let mut update_ids = Vec::new();

        for asset in assets {
            self.classify_sync_asset(
                &asset,
                &job.import_paths,
                &job.exclusion_patterns,
                &mut active_offline,
                &mut trashed_offline,
                &mut active_online,
                &mut trashed_online,
                &mut update_ids,
            )
            .await;
        }

        library::mark_assets_offline(&self.pool, &active_offline, false)
            .await
            .map_err(|err| err.to_string())?;
        library::mark_assets_offline(&self.pool, &trashed_offline, true)
            .await
            .map_err(|err| err.to_string())?;
        library::mark_assets_online(&self.pool, &active_online, false)
            .await
            .map_err(|err| err.to_string())?;
        library::mark_assets_online(&self.pool, &trashed_online, true)
            .await
            .map_err(|err| err.to_string())?;

        self.queue_post_sync_jobs(&update_ids).await?;

        Ok(())
    }

    async fn queue_post_sync_jobs(&self, asset_ids: &[Uuid]) -> Result<(), String> {
        for asset_id in asset_ids {
            self.jobs
                .queue_sidecar_check_with_source(asset_id, Some("upload"))
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    async fn handle_remove_asset(&self, job: LibraryRemoveAssetJob) -> Result<(), String> {
        for asset_path in job.paths {
            let Some(asset_id) =
                library::get_asset_id_by_library_path(&self.pool, &job.library_id, &asset_path)
                    .await
                    .map_err(|err| err.to_string())?
            else {
                continue;
            };
            library::remove_asset_by_id(&self.pool, &asset_id)
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    async fn classify_sync_asset(
        &self,
        asset: &LibraryAssetSyncRow,
        import_paths: &[String],
        exclusion_patterns: &[String],
        active_offline: &mut Vec<Uuid>,
        trashed_offline: &mut Vec<Uuid>,
        active_online: &mut Vec<Uuid>,
        trashed_online: &mut Vec<Uuid>,
        update_ids: &mut Vec<Uuid>,
    ) {
        let metadata = tokio::fs::metadata(&asset.original_path).await;
        let trashed = asset.status == "trashed";

        match metadata {
            Err(_) => {
                if asset.is_offline {
                    return;
                }
                if trashed {
                    trashed_offline.push(asset.id);
                } else {
                    active_offline.push(asset.id);
                }
            }
            Ok(meta) => {
                if asset.is_offline && asset.status != "deleted" {
                    let in_import = import_paths
                        .iter()
                        .any(|path| asset.original_path.starts_with(path));
                    if !in_import {
                        return;
                    }
                    if path_matches_exclusion(&asset.original_path, exclusion_patterns) {
                        return;
                    }
                    if trashed {
                        trashed_online.push(asset.id);
                    } else {
                        active_online.push(asset.id);
                    }
                    return;
                }

                let modified = DateTime::<Utc>::from(
                    meta.modified()
                        .unwrap_or_else(|_| std::time::SystemTime::now()),
                );
                if modified.timestamp_millis() != asset.file_modified_at.timestamp_millis() {
                    update_ids.push(asset.id);
                }
            }
        }
    }

    async fn validate_import_paths(&self, library: &LibraryRow) -> Result<Vec<String>, String> {
        let mut valid = Vec::new();
        for import_path in &library.import_paths {
            if self.storage.is_immich_path(import_path) {
                eprintln!(
                    "Skipping invalid import path {import_path}: cannot use media upload folder"
                );
                continue;
            }
            if !Path::new(import_path).is_absolute() {
                eprintln!("Skipping invalid import path {import_path}: path must be absolute");
                continue;
            }
            let path = PathBuf::from(import_path);
            match tokio::fs::metadata(&path).await {
                Ok(meta) if meta.is_dir() => {
                    valid.push(path.to_string_lossy().into_owned());
                }
                Ok(_) => eprintln!("Skipping invalid import path {import_path}: not a directory"),
                Err(err) => {
                    eprintln!("Skipping invalid import path {import_path}: {err}");
                }
            }
        }
        Ok(valid)
    }

    async fn import_external_file(
        &self,
        file_path: &str,
        owner_id: &Uuid,
        library_id: &Uuid,
    ) -> Result<Option<Uuid>, String> {
        let normalized = Path::new(file_path).to_string_lossy().into_owned();
        let metadata = tokio::fs::metadata(&normalized)
            .await
            .map_err(|err| err.to_string())?;
        if !metadata.is_file() {
            return Ok(None);
        }

        let modified = DateTime::<Utc>::from(
            metadata
                .modified()
                .unwrap_or_else(|_| std::time::SystemTime::now()),
        );
        let file_name = Path::new(&normalized)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let asset_type = if is_video_path(&normalized) {
            "VIDEO"
        } else {
            "IMAGE"
        };
        let checksum = sha1_bytes(format!("path:{normalized}").as_bytes());

        let asset_id = assets::create_library_asset(
            &self.pool,
            NewLibraryAsset {
                owner_id: *owner_id,
                library_id: *library_id,
                asset_type,
                original_path: &normalized,
                checksum: &checksum,
                file_created_at: modified,
                file_modified_at: modified,
                original_file_name: file_name,
            },
        )
        .await
        .map_err(|err| err.to_string())?;

        Ok(Some(asset_id))
    }
}

fn is_hidden_path(path: &str) -> bool {
    Path::new(path)
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .any(|part| part.starts_with('.'))
}

pub fn spawn(
    pool: PgPool,
    redis_url: String,
    storage: StoragePaths,
    jobs: JobService,
    concurrency: usize,
) {
    tokio::spawn(async move {
        let processor = Arc::new(LibraryProcessor::new(pool, storage, jobs));
        let worker = WorkerBuilder::new(QUEUE_LIBRARY)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    let job_name = job.name.clone();
                    crate::service::workers::wrap_simple_job(QUEUE_LIBRARY, &job_name, || async {
                        processor
                            .process(&job_name, &job.data)
                            .await
                            .map_err(|err| err.to_string())
                    })
                    .await
                }
            })
            .await;

        match handle {
            Ok(worker_handle) => {
                crate::service::worker_registry::register(worker_handle);
                std::future::pending::<()>().await;
            }
            Err(err) => {
                eprintln!("library worker failed to start: {err}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sync_files_job_deserializes_camel_case() {
        let job: LibrarySyncFilesJob = serde_json::from_value(json!({
            "libraryId": "11111111-1111-1111-1111-111111111111",
            "paths": ["/data/photo.jpg"],
            "progressCounter": 3,
            "totalAssets": 10
        }))
        .expect("libraryId payload should deserialize");

        assert_eq!(
            job.library_id.to_string(),
            "11111111-1111-1111-1111-111111111111"
        );
        assert_eq!(job.paths, vec!["/data/photo.jpg"]);
        assert_eq!(job.progress_counter, Some(3));
        assert_eq!(job.total_assets, Some(10));
    }

    #[test]
    fn sync_assets_job_deserializes_camel_case() {
        let job: LibrarySyncAssetsJob = serde_json::from_value(json!({
            "libraryId": "11111111-1111-1111-1111-111111111111",
            "importPaths": ["/data"],
            "exclusionPatterns": ["**/.git/**"],
            "assetIds": ["22222222-2222-2222-2222-222222222222"]
        }))
        .expect("libraryId payload should deserialize");

        assert_eq!(job.import_paths, vec!["/data"]);
        assert_eq!(job.exclusion_patterns, vec!["**/.git/**"]);
        assert_eq!(job.asset_ids.len(), 1);
    }
}
