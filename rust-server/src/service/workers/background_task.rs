use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::asset_delete;
use crate::models::db::maintenance;
use crate::models::db::move_history;
use crate::models::db::stack;
use crate::models::db::trash;
use crate::models::db::users::UserDb;
use crate::models::dto::env::EnvDto;
use crate::service::job::JobService;
use crate::service::sync::MAX_DAYS;
use crate::service::version_check;
use crate::service::websocket::WebSocketHub;
use crate::utils::storage::StoragePaths;
use crate::utils::system_config::get_merged;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_BACKGROUND: &str = "backgroundTask";
const JOBS_ASSET_PAGINATION_SIZE: usize = 1000;

#[derive(Debug, Deserialize)]
struct FileDeleteJob {
    files: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntityJob {
    id: Uuid,
    #[serde(default)]
    delete_on_disk: Option<bool>,
    #[serde(default)]
    force: Option<bool>,
}

#[derive(Clone)]
pub struct BackgroundTaskProcessor {
    pool: PgPool,
    websocket: WebSocketHub,
    env: EnvDto,
    storage: StoragePaths,
    jobs: JobService,
}

impl BackgroundTaskProcessor {
    pub fn new(
        pool: PgPool,
        websocket: WebSocketHub,
        env: EnvDto,
        storage: StoragePaths,
        jobs: JobService,
    ) -> Self {
        Self {
            pool,
            websocket,
            env,
            storage,
            jobs,
        }
    }

    pub async fn process(&self, name: &str, data: &Value) -> Result<(), String> {
        match name {
            "FileDelete" => self.handle_file_delete(data).await,
            "SessionCleanup" => self.handle_session_cleanup().await,
            "NotificationsCleanup" => self.handle_notifications_cleanup().await,
            "PersonCleanup" => self.handle_person_cleanup().await,
            "TagCleanup" => self.handle_tag_cleanup().await,
            "MemoryCleanup" => self.handle_memory_cleanup().await,
            "VersionCheck" => self.handle_version_check().await,
            "UserSyncUsage" => self.handle_user_sync_usage().await,
            "AuditTableCleanup" => self.handle_audit_table_cleanup().await,
            "HlsSessionCleanup" => self.handle_hls_session_cleanup().await,
            "AssetDeleteCheck" => self.handle_asset_delete_check().await,
            "AssetEmptyTrash" => self.handle_asset_empty_trash().await,
            "AssetDelete" => {
                let job: EntityJob =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                self.handle_asset_delete(job).await
            }
            "UserDeleteCheck" => self.handle_user_delete_check().await,
            "UserDelete" => {
                let job: EntityJob =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                self.handle_user_delete(job).await
            }
            "MemoryGenerate" => self.handle_memory_generate().await,
            other => {
                eprintln!(
                    "backgroundTask job {other} is not implemented in rust-server yet; skipping"
                );
                Ok(())
            }
        }
    }

    async fn handle_file_delete(&self, data: &Value) -> Result<(), String> {
        let job: FileDeleteJob =
            serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
        for file in job.files {
            if file.is_empty() {
                continue;
            }
            if let Err(err) = tokio::fs::remove_file(&file).await {
                eprintln!("unable to remove file from disk ({file}): {err}");
            }
        }
        Ok(())
    }

    async fn handle_session_cleanup(&self) -> Result<(), String> {
        let deleted = crate::models::db::sessions::SessionPO::cleanup_expired(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if deleted > 0 {
            println!("deleted {deleted} expired session token(s)");
        }
        Ok(())
    }

    async fn handle_notifications_cleanup(&self) -> Result<(), String> {
        crate::models::db::notification::cleanup_old(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn handle_person_cleanup(&self) -> Result<(), String> {
        let people = crate::models::db::person::list_without_faces(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if people.is_empty() {
            return Ok(());
        }

        for (_, thumbnail_path) in &people {
            if !thumbnail_path.is_empty() {
                let _ = tokio::fs::remove_file(thumbnail_path).await;
            }
        }

        let ids: Vec<Uuid> = people.into_iter().map(|(id, _)| id).collect();
        crate::models::db::person::delete_by_ids(&self.pool, &ids)
            .await
            .map_err(|err| err.to_string())?;
        println!("deleted {} people without faces", ids.len());
        Ok(())
    }

    async fn handle_tag_cleanup(&self) -> Result<(), String> {
        let deleted = maintenance::delete_empty_tags(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if deleted > 0 {
            println!("deleted {deleted} empty tags");
        }
        Ok(())
    }

    async fn handle_memory_cleanup(&self) -> Result<(), String> {
        crate::models::db::memory::cleanup_stale(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn handle_memory_generate(&self) -> Result<(), String> {
        crate::service::memory_generate::run_memory_generate(&self.pool).await
    }

    async fn handle_version_check(&self) -> Result<(), String> {
        version_check::run_version_check(
            &self.pool,
            &self.websocket,
            self.env.immich_env.as_ref(),
        )
        .await
    }

    async fn handle_user_sync_usage(&self) -> Result<(), String> {
        let updated = maintenance::sync_all_user_usage(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        println!("synced quota usage for {updated} user(s)");
        Ok(())
    }

    async fn handle_audit_table_cleanup(&self) -> Result<(), String> {
        let prune_threshold = MAX_DAYS + 1;
        let deleted = maintenance::cleanup_audit_tables(&self.pool, prune_threshold)
            .await
            .map_err(|err| err.to_string())?;
        if deleted > 0 {
            println!("deleted {deleted} stale audit row(s)");
        }
        Ok(())
    }

    async fn handle_hls_session_cleanup(&self) -> Result<(), String> {
        use crate::models::db::advisory_lock::{self, LOCK_HLS_SESSION_CLEANUP};

        let result = advisory_lock::run_with_lock(&self.pool, LOCK_HLS_SESSION_CLEANUP, || async {
            let sessions = maintenance::list_expired_hls_sessions(&self.pool)
                .await
                .map_err(|err| err.to_string())?;

            for session in sessions {
                let dir = self
                    .storage
                    .hls_session_folder(&session.owner_id, &session.id);
                if let Err(err) = tokio::fs::remove_dir_all(&dir).await {
                    if err.kind() != std::io::ErrorKind::NotFound {
                        eprintln!("failed to remove HLS session dir {}: {err}", dir.display());
                    }
                }
                maintenance::delete_hls_session(&self.pool, &session.id)
                    .await
                    .map_err(|err| err.to_string())?;
            }

            Ok(())
        })
        .await
        .map_err(|err| err.to_string())?;

        result
    }

    async fn handle_asset_delete_check(&self) -> Result<(), String> {
        let config = get_merged(&self.pool).await.map_err(|err| err.to_string())?;
        let trash_enabled = config
            .get("trash")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let trash_days = if trash_enabled {
            config
                .get("trash")
                .and_then(|value| value.get("days"))
                .and_then(|value| value.as_i64())
                .unwrap_or(30) as i32
        } else {
            0
        };

        let before = Utc::now() - Duration::days(trash_days as i64);
        let assets = asset_delete::list_trashed_before(&self.pool, before)
            .await
            .map_err(|err| err.to_string())?;

        for chunk in assets.chunks(JOBS_ASSET_PAGINATION_SIZE) {
            for (asset_id, is_offline) in chunk {
                self.jobs
                    .queue_json_job(
                        QUEUE_BACKGROUND,
                        "AssetDelete",
                        serde_json::json!({
                            "id": asset_id,
                            "deleteOnDisk": !is_offline,
                        }),
                    )
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        Ok(())
    }

    async fn handle_asset_empty_trash(&self) -> Result<(), String> {
        let asset_ids = trash::list_deleted_ids(&self.pool)
            .await
            .map_err(|err| err.to_string())?;

        for chunk in asset_ids.chunks(JOBS_ASSET_PAGINATION_SIZE) {
            for asset_id in chunk {
                self.jobs
                    .queue_json_job(
                        QUEUE_BACKGROUND,
                        "AssetDelete",
                        serde_json::json!({
                            "id": asset_id,
                            "deleteOnDisk": true,
                        }),
                    )
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        if !asset_ids.is_empty() {
            println!("Queued {} asset(s) for deletion from the trash", asset_ids.len());
        }
        Ok(())
    }

    async fn handle_asset_delete(&self, job: EntityJob) -> Result<(), String> {
        let delete_on_disk = job.delete_on_disk.unwrap_or(false);
        let Some(asset) = asset_delete::get_for_deletion(&self.pool, &job.id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(());
        };

        if let (Some(stack_id), Some(primary_asset_id)) = (asset.stack_id, asset.primary_asset_id) {
            if primary_asset_id == asset.id {
                let replacements =
                    asset_delete::list_stack_timeline_asset_ids(&self.pool, &stack_id, &asset.id)
                        .await
                        .map_err(|err| err.to_string())?;
                if replacements.len() >= 2 {
                    stack::update_primary(&self.pool, &stack_id, &replacements[0])
                        .await
                        .map_err(|err| err.to_string())?;
                } else {
                    stack::delete(&self.pool, &stack_id)
                        .await
                        .map_err(|err| err.to_string())?;
                }
            }
        }

        let file_paths = asset_delete::list_asset_file_paths(&self.pool, &job.id)
            .await
            .map_err(|err| err.to_string())?;
        let live_photo_video_id = asset.live_photo_video_id;

        if asset.library_id.is_none() {
            if let Some(size) = asset.file_size {
                crate::models::db::assets::update_quota_usage(
                    &self.pool,
                    &asset.owner_id,
                    -size,
                )
                .await
                .map_err(|err| err.to_string())?;
            }
        }

        asset_delete::hard_delete(&self.pool, &job.id)
            .await
            .map_err(|err| err.to_string())?;

        move_history::clean_move_history_single(&self.pool, &job.id)
            .await
            .map_err(|err| err.to_string())?;

        let mut files_to_delete = file_paths;
        if delete_on_disk && !asset.is_offline {
            files_to_delete.push(asset.original_path.clone());
        }
        files_to_delete.retain(|path| !path.is_empty());
        self.jobs
            .queue_file_delete(&files_to_delete)
            .await
            .map_err(|err| err.to_string())?;

        if let Some(video_id) = live_photo_video_id {
            let count = asset_delete::count_live_photo_references(&self.pool, &video_id)
                .await
                .map_err(|err| err.to_string())?;
            if count == 0 {
                self.jobs
                    .queue_json_job(
                        QUEUE_BACKGROUND,
                        "AssetDelete",
                        serde_json::json!({
                            "id": video_id,
                            "deleteOnDisk": delete_on_disk,
                        }),
                    )
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        Ok(())
    }

    async fn handle_user_delete_check(&self) -> Result<(), String> {
        let config = get_merged(&self.pool).await.map_err(|err| err.to_string())?;
        let delete_delay = config
            .get("user")
            .and_then(|value| value.get("deleteDelay"))
            .and_then(|value| value.as_i64())
            .unwrap_or(7) as i32;

        let before = Utc::now() - Duration::days(delete_delay as i64);
        let user_ids = UserDb::list_deleted_before(&self.pool, before)
            .await
            .map_err(|err| err.to_string())?;

        for user_id in user_ids {
            self.jobs
                .queue_json_job(
                    QUEUE_BACKGROUND,
                    "UserDelete",
                    serde_json::json!({ "id": user_id }),
                )
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(())
    }

    async fn handle_user_delete(&self, job: EntityJob) -> Result<(), String> {
        let force = job.force.unwrap_or(false);
        let user = UserDb::select_by_id_admin(&self.pool, &job.id, true)
            .await
            .map_err(|err| err.to_string())?
            .ok_or_else(|| format!("User {} not found", job.id))?;

        if !force {
            let config = get_merged(&self.pool).await.map_err(|err| err.to_string())?;
            let delete_delay = config
                .get("user")
                .and_then(|value| value.get("deleteDelay"))
                .and_then(|value| value.as_i64())
                .unwrap_or(7) as i32;
            if let Some(deleted_at) = user.deleted_at {
                let ready_before = Utc::now() - Duration::days(delete_delay as i64);
                if deleted_at > ready_before {
                    eprintln!("Skipped user not ready for deletion: id={}", job.id);
                    return Ok(());
                }
            } else {
                eprintln!("Skipped user not ready for deletion: id={}", job.id);
                return Ok(());
            }
        }

        println!("Deleting user: {}", user.id);

        let folders = [
            self.storage
                .library_folder(&user.id, user.storage_label.as_deref()),
            self.storage.user_upload_folder(&user.id),
            self.storage.user_profile_folder(&user.id),
            self.storage.user_thumbs_folder(&user.id),
            self.storage.user_encoded_video_folder(&user.id),
        ];

        for folder in folders {
            if let Err(err) = tokio::fs::remove_dir_all(&folder).await {
                if err.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("failed to remove user folder {}: {err}", folder.display());
                }
            }
        }

        UserDb::hard_delete(&self.pool, &user.id)
            .await
            .map_err(|err| err.to_string())?;

        Ok(())
    }
}

pub fn spawn(
    pool: PgPool,
    redis_url: String,
    websocket: WebSocketHub,
    env: EnvDto,
    storage: StoragePaths,
    jobs: JobService,
    concurrency: usize,
) {
    tokio::spawn(async move {
        let processor = Arc::new(BackgroundTaskProcessor::new(
            pool, websocket, env, storage, jobs,
        ));
        let worker = WorkerBuilder::new(QUEUE_BACKGROUND)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    let job_name = job.name.clone();
                    crate::service::workers::wrap_simple_job(QUEUE_BACKGROUND, &job_name, || async {
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
                eprintln!("backgroundTask worker failed to start: {err}");
            }
        }
    });
}
