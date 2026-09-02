use std::collections::HashSet;
use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::assets;
use crate::models::db::integrity::{
    self, AssetPathItemRow, IntegrityReportDeleteRow, IntegrityReportInsert,
    CHECKSUM_CHECKPOINT_KEY, JOBS_INTEGRITY_BATCH_SIZE, REPORT_TYPE_CHECKSUM,
    REPORT_TYPE_MISSING, REPORT_TYPE_UNTRACKED,
};
use crate::models::db::system_metadata::{get_json, set_json};
use crate::service::job::JobService;
use crate::utils::checksum::sha1_bytes;
use crate::utils::file_walk::walk_file_batches;
use crate::utils::mime_types::supported_file_extensions;
use crate::utils::storage::StoragePaths;
use crate::utils::system_config::get_merged;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_INTEGRITY: &str = "integrityCheck";

#[derive(Clone)]
pub struct IntegrityProcessor {
    pool: PgPool,
    storage: StoragePaths,
    jobs: JobService,
}

#[derive(Debug, Deserialize)]
struct IntegrityJob {
    #[serde(default)]
    refresh_only: bool,
}

#[derive(Debug, Deserialize)]
struct UntrackedFilesJob {
    #[serde(rename = "type")]
    batch_type: String,
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PathReportItem {
    report_id: Uuid,
    path: String,
}

#[derive(Debug, Deserialize)]
struct PathReportRefreshJob {
    items: Vec<PathReportItem>,
}

#[derive(Debug, Deserialize)]
struct MissingFileItem {
    path: String,
    #[serde(default)]
    asset_id: Option<Uuid>,
    #[serde(default)]
    file_asset_id: Option<Uuid>,
    #[serde(default)]
    report_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct MissingFilesJob {
    items: Vec<MissingFileItem>,
}

#[derive(Debug, Deserialize)]
struct ChecksumRefreshItem {
    report_id: Uuid,
    path: String,
    #[serde(default)]
    checksum: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChecksumRefreshJob {
    items: Vec<ChecksumRefreshItem>,
}

#[derive(Debug, Deserialize)]
struct DeleteReportTypeJob {
    #[serde(default)]
    r#type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeleteReportItem {
    id: Uuid,
    path: String,
    #[serde(default)]
    asset_id: Option<Uuid>,
    #[serde(default)]
    file_asset_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct DeleteReportsJob {
    reports: Vec<DeleteReportItem>,
}

impl IntegrityProcessor {
    pub fn new(pool: PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self { pool, storage, jobs }
    }

    pub async fn process(&self, name: &str, data: &Value) -> Result<(), String> {
        match name {
            "IntegrityUntrackedFilesQueueAll" => {
                self.handle_untracked_files_queue_all(data).await
            }
            "IntegrityUntrackedFiles" => self.handle_untracked_files(data).await,
            "IntegrityUntrackedFilesRefresh" | "IntegrityUntrackedRefresh" => {
                self.handle_untracked_refresh(data).await
            }
            "IntegrityMissingFilesQueueAll" => {
                self.handle_missing_files_queue_all(data).await
            }
            "IntegrityMissingFiles" => self.handle_missing_files(data).await,
            "IntegrityMissingFilesRefresh" => self.handle_missing_refresh(data).await,
            "IntegrityChecksumFiles" => self.handle_checksum_files(data).await,
            "IntegrityChecksumFilesRefresh" => self.handle_checksum_refresh(data).await,
            "IntegrityDeleteReportType" => self.handle_delete_report_type(data).await,
            "IntegrityDeleteReports" => self.handle_delete_reports(data).await,
            other => {
                eprintln!("integrityCheck job {other} is not implemented; skipping");
                Ok(())
            }
        }
    }

    async fn handle_untracked_files_queue_all(&self, data: &Value) -> Result<(), String> {
        let job: IntegrityJob =
            serde_json::from_value(data.clone()).unwrap_or(IntegrityJob { refresh_only: false });

        self.queue_refresh_all_untracked_files().await?;

        if job.refresh_only {
            println!("integrity: untracked refresh complete");
            return Ok(());
        }

        println!("integrity: scanning for untracked files");
        let extensions = supported_file_extensions();
        let asset_roots = vec![
            self.storage.encoded_video_base(),
            self.storage.library_base(),
            self.storage.upload_base(),
        ];
        let thumb_roots = vec![self.storage.thumbs_base()];

        let asset_batches = tokio::task::spawn_blocking({
            let roots = asset_roots.clone();
            let extensions = extensions.clone();
            move || walk_file_batches(&roots, Some(&extensions), JOBS_INTEGRITY_BATCH_SIZE as usize)
        })
        .await
        .map_err(|err| err.to_string())?;

        let mut total = 0usize;
        for batch in asset_batches {
            let count = batch.len();
            total += count;
            self.queue_job(
                "IntegrityUntrackedFiles",
                json!({ "type": "asset", "paths": batch }),
            )
            .await?;
            println!("integrity: queued untracked check of {count} asset file(s) ({total} so far)");
        }

        let thumb_batches = tokio::task::spawn_blocking({
            let roots = thumb_roots;
            move || walk_file_batches(&roots, None, JOBS_INTEGRITY_BATCH_SIZE as usize)
        })
        .await
        .map_err(|err| err.to_string())?;

        for batch in thumb_batches {
            let count = batch.len();
            total += count;
            self.queue_job(
                "IntegrityUntrackedFiles",
                json!({ "type": "asset_file", "paths": batch }),
            )
            .await?;
            println!(
                "integrity: queued untracked check of {count} thumbnail file(s) ({total} so far)"
            );
        }

        Ok(())
    }

    async fn handle_untracked_files(&self, data: &Value) -> Result<(), String> {
        let job: UntrackedFilesJob =
            serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;

        let mut untracked: HashSet<String> = job.paths.into_iter().collect();
        if job.batch_type == "asset" {
            let rows = integrity::get_asset_paths_by_paths(&self.pool, &untracked.iter().cloned().collect::<Vec<_>>())
                .await
                .map_err(|err| err.to_string())?;
            for row in rows {
                untracked.remove(&row.original_path);
                if let Some(path) = row.encoded_video_path {
                    untracked.remove(&path);
                }
            }
        } else {
            let rows = integrity::get_asset_file_paths_by_paths(
                &self.pool,
                &untracked.iter().cloned().collect::<Vec<_>>(),
            )
            .await
            .map_err(|err| err.to_string())?;
            for row in rows {
                untracked.remove(&row.path);
            }
        }

        let person_rows = integrity::get_person_thumbnail_paths_by_paths(
            &self.pool,
            &untracked.iter().cloned().collect::<Vec<_>>(),
        )
        .await
        .map_err(|err| err.to_string())?;
        for row in person_rows {
            untracked.remove(&row.thumbnail_path);
        }

        if !untracked.is_empty() {
            let reports: Vec<IntegrityReportInsert> = untracked
                .into_iter()
                .map(|path| IntegrityReportInsert {
                    report_type: REPORT_TYPE_UNTRACKED.to_string(),
                    path,
                    asset_id: None,
                    file_asset_id: None,
                })
                .collect();
            integrity::create_reports(&self.pool, &reports)
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(())
    }

    async fn handle_untracked_refresh(&self, data: &Value) -> Result<(), String> {
        let job: PathReportRefreshJob =
            serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;

        if job.items.is_empty() {
            return Ok(());
        }

        let paths: Vec<String> = job.items.iter().map(|item| item.path.clone()).collect();
        let tracked_rows = integrity::get_tracked_paths(&self.pool, &paths)
            .await
            .map_err(|err| err.to_string())?;
        let tracked_paths: HashSet<String> = tracked_rows.into_iter().map(|row| row.path).collect();

        let mut stale = Vec::new();
        for item in job.items {
            if tracked_paths.contains(&item.path) {
                stale.push(item.report_id);
                continue;
            }

            if tokio::fs::metadata(&item.path).await.is_err() {
                stale.push(item.report_id);
            }
        }

        integrity::delete_by_ids(&self.pool, &stale)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn handle_missing_files_queue_all(&self, data: &Value) -> Result<(), String> {
        let job: IntegrityJob =
            serde_json::from_value(data.clone()).unwrap_or(IntegrityJob { refresh_only: false });

        if job.refresh_only {
            self.queue_refresh_all_missing_files().await?;
            return Ok(());
        }

        println!("integrity: scanning for missing files");
        let mut offset = 0i64;
        let mut total = 0i64;
        loop {
            let rows = integrity::stream_asset_paths_page(
                &self.pool,
                offset,
                JOBS_INTEGRITY_BATCH_SIZE,
            )
            .await
            .map_err(|err| err.to_string())?;
            if rows.is_empty() {
                break;
            }
            let count = rows.len() as i64;
            offset += count;
            total += count;
            self.queue_job(
                "IntegrityMissingFiles",
                json!({ "items": rows_to_missing_items(rows) }),
            )
            .await?;
            println!("integrity: queued missing check of {count} file(s) ({total} so far)");
            if count < JOBS_INTEGRITY_BATCH_SIZE {
                break;
            }
        }

        Ok(())
    }

    async fn handle_missing_files(&self, data: &Value) -> Result<(), String> {
        let job: MissingFilesJob =
            serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;

        let mut outdated = Vec::new();
        let mut missing = Vec::new();

        for item in job.items {
            match tokio::fs::metadata(&item.path).await {
                Ok(_) => {
                    if let Some(report_id) = item.report_id {
                        outdated.push(report_id);
                    }
                }
                Err(_) => missing.push(IntegrityReportInsert {
                    report_type: REPORT_TYPE_MISSING.to_string(),
                    path: item.path,
                    asset_id: item.asset_id,
                    file_asset_id: item.file_asset_id,
                }),
            }
        }

        integrity::delete_by_ids(&self.pool, &outdated)
            .await
            .map_err(|err| err.to_string())?;
        if !missing.is_empty() {
            integrity::create_reports(&self.pool, &missing)
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(())
    }

    async fn handle_missing_refresh(&self, data: &Value) -> Result<(), String> {
        let job: PathReportRefreshJob =
            serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;

        let mut stale = Vec::new();
        for item in job.items {
            if tokio::fs::metadata(&item.path).await.is_ok() {
                stale.push(item.report_id);
            }
        }

        integrity::delete_by_ids(&self.pool, &stale)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn handle_checksum_files(&self, data: &Value) -> Result<(), String> {
        let job: IntegrityJob =
            serde_json::from_value(data.clone()).unwrap_or(IntegrityJob { refresh_only: false });

        if job.refresh_only {
            self.queue_refresh_all_checksum_files().await?;
            return Ok(());
        }

        let config = get_merged(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        let checksum_cfg = config
            .get("integrityChecks")
            .and_then(|value| value.get("checksumFiles"));
        let time_limit = checksum_cfg
            .and_then(|value| value.get("timeLimit"))
            .and_then(|value| value.as_i64())
            .unwrap_or(3_600_000);
        let percentage_limit = checksum_cfg
            .and_then(|value| value.get("percentageLimit"))
            .and_then(|value| value.as_f64())
            .unwrap_or(1.0);

        let total_assets = integrity::get_asset_count(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        let mut start_marker = load_checksum_checkpoint(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        let mut end_marker: Option<DateTime<Utc>> = None;
        let started_at = std::time::Instant::now();
        let mut processed = 0i64;
        let mut last_created_at: Option<DateTime<Utc>> = None;

        loop {
            let mut offset = 0i64;
            loop {
                let rows = integrity::stream_asset_checksums_page(
                    &self.pool,
                    start_marker,
                    end_marker,
                    offset,
                    JOBS_INTEGRITY_BATCH_SIZE,
                )
                .await
                .map_err(|err| err.to_string())?;
                if rows.is_empty() {
                    break;
                }

                let batch_len = rows.len() as i64;
                for row in rows {
                    self.check_asset_checksum(&row).await?;
                    processed += 1;
                    last_created_at = Some(row.created_at);

                    if processed % 100 == 0 {
                        let elapsed = started_at.elapsed().as_millis() as i64;
                        let avg = elapsed / processed.max(1);
                        let progress = if total_assets > 0 {
                            (processed as f64 / total_assets as f64) * 100.0
                        } else {
                            100.0
                        };
                        println!(
                            "integrity: processed {processed} checksums (avg {avg} ms/asset, {progress:.2}% complete)"
                        );
                    }

                    if started_at.elapsed().as_millis() as i64 > time_limit
                        || (total_assets > 0
                            && processed as f64 > total_assets as f64 * percentage_limit)
                    {
                        save_checksum_checkpoint(&self.pool, last_created_at)
                            .await
                            .map_err(|err| err.to_string())?;
                        println!("integrity: reached checksum stop criteria");
                        return Ok(());
                    }
                }

                offset += batch_len;
                if batch_len < JOBS_INTEGRITY_BATCH_SIZE {
                    break;
                }
            }

            if end_marker.is_some() {
                break;
            }
            if start_marker.is_none() {
                break;
            }
            end_marker = start_marker;
            start_marker = None;
        }

        save_checksum_checkpoint(&self.pool, last_created_at)
            .await
            .map_err(|err| err.to_string())?;

        if last_created_at.is_some() {
            println!(
                "integrity: checksum job will continue from {:?}",
                last_created_at
            );
        } else {
            println!("integrity: checksum job covered all assets");
        }

        Ok(())
    }

    async fn check_asset_checksum(
        &self,
        row: &integrity::AssetChecksumRow,
    ) -> Result<(), String> {
        match tokio::fs::read(&row.original_path).await {
            Ok(bytes) => {
                let hash = sha1_bytes(&bytes);
                if hash == row.checksum {
                    if let Some(report_id) = row.report_id {
                        integrity::delete_by_ids(&self.pool, &[report_id])
                            .await
                            .map_err(|err| err.to_string())?;
                    }
                } else {
                    integrity::create_reports(
                        &self.pool,
                        &[IntegrityReportInsert {
                            report_type: REPORT_TYPE_CHECKSUM.to_string(),
                            path: row.original_path.clone(),
                            asset_id: Some(row.asset_id),
                            file_asset_id: None,
                        }],
                    )
                    .await
                    .map_err(|err| err.to_string())?;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                if let Some(report_id) = row.report_id {
                    integrity::delete_by_ids(&self.pool, &[report_id])
                        .await
                        .map_err(|err| err.to_string())?;
                }
            }
            Err(err) => {
                eprintln!(
                    "integrity: failed to checksum {}: {err}",
                    row.original_path
                );
                integrity::create_reports(
                    &self.pool,
                    &[IntegrityReportInsert {
                        report_type: REPORT_TYPE_CHECKSUM.to_string(),
                        path: row.original_path.clone(),
                        asset_id: Some(row.asset_id),
                        file_asset_id: None,
                    }],
                )
                .await
                .map_err(|err| err.to_string())?;
            }
        }
        Ok(())
    }

    async fn handle_checksum_refresh(&self, data: &Value) -> Result<(), String> {
        let job: ChecksumRefreshJob =
            serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;

        let mut stale = Vec::new();
        for item in job.items {
            let Some(checksum_hex) = item.checksum else {
                stale.push(item.report_id);
                continue;
            };
            let Ok(expected) = hex::decode(checksum_hex) else {
                stale.push(item.report_id);
                continue;
            };

            match tokio::fs::read(&item.path).await {
                Ok(bytes) => {
                    if sha1_bytes(&bytes) == expected {
                        stale.push(item.report_id);
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    stale.push(item.report_id);
                }
                Err(_) => {}
            }
        }

        integrity::delete_by_ids(&self.pool, &stale)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn handle_delete_report_type(&self, data: &Value) -> Result<(), String> {
        let job: DeleteReportTypeJob =
            serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;

        let properties: Vec<Option<&str>> = match job.r#type.as_deref() {
            Some(REPORT_TYPE_CHECKSUM) => vec![Some("assetId")],
            Some(REPORT_TYPE_MISSING) => vec![Some("assetId"), Some("fileAssetId")],
            Some(REPORT_TYPE_UNTRACKED) => vec![None],
            _ => vec![None, Some("assetId"), Some("fileAssetId")],
        };

        for property in properties {
            let mut offset = 0i64;
            loop {
                let rows = integrity::stream_integrity_reports_by_property_page(
                    &self.pool,
                    property,
                    job.r#type.as_deref(),
                    offset,
                    JOBS_INTEGRITY_BATCH_SIZE,
                )
                .await
                .map_err(|err| err.to_string())?;
                if rows.is_empty() {
                    break;
                }
                let count = rows.len();
                offset += count as i64;
                self.queue_job(
                    "IntegrityDeleteReports",
                    json!({ "reports": rows_to_delete_items(rows) }),
                )
                .await?;
                if count < JOBS_INTEGRITY_BATCH_SIZE as usize {
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_delete_reports(&self, data: &Value) -> Result<(), String> {
        let job: DeleteReportsJob =
            serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;

        let asset_ids: Vec<Uuid> = job
            .reports
            .iter()
            .filter_map(|report| report.asset_id)
            .collect();
        if !asset_ids.is_empty() {
            assets::trash_assets(&self.pool, &asset_ids, false)
                .await
                .map_err(|err| err.to_string())?;
            let report_ids: Vec<Uuid> = job
                .reports
                .iter()
                .filter(|report| report.asset_id.is_some())
                .map(|report| report.id)
                .collect();
            integrity::delete_by_ids(&self.pool, &report_ids)
                .await
                .map_err(|err| err.to_string())?;
        }

        for report in job
            .reports
            .iter()
            .filter(|report| report.file_asset_id.is_some() && report.asset_id.is_none())
        {
            if let Some(file_asset_id) = report.file_asset_id {
                integrity::delete_asset_file(&self.pool, &file_asset_id)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        let path_reports: Vec<&DeleteReportItem> = job
            .reports
            .iter()
            .filter(|report| report.asset_id.is_none() && report.file_asset_id.is_none())
            .collect();
        if !path_reports.is_empty() {
            let paths: Vec<String> = path_reports.iter().map(|report| report.path.clone()).collect();
            let tracked_rows = integrity::get_tracked_paths(&self.pool, &paths)
                .await
                .map_err(|err| err.to_string())?;
            let tracked_paths: HashSet<String> =
                tracked_rows.into_iter().map(|row| row.path).collect();

            for report in &path_reports {
                if !tracked_paths.contains(&report.path) {
                    let _ = tokio::fs::remove_file(&report.path).await;
                }
            }

            let ids: Vec<Uuid> = path_reports.iter().map(|report| report.id).collect();
            integrity::delete_by_ids(&self.pool, &ids)
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(())
    }

    async fn queue_refresh_all_untracked_files(&self) -> Result<(), String> {
        self.queue_report_refresh_batches(REPORT_TYPE_UNTRACKED, "IntegrityUntrackedRefresh")
            .await
    }

    async fn queue_refresh_all_missing_files(&self) -> Result<(), String> {
        self.queue_report_refresh_batches(REPORT_TYPE_MISSING, "IntegrityMissingFilesRefresh")
            .await
    }

    async fn queue_refresh_all_checksum_files(&self) -> Result<(), String> {
        let mut offset = 0i64;
        loop {
            let rows = integrity::stream_integrity_reports_with_checksum_page(
                &self.pool,
                REPORT_TYPE_CHECKSUM,
                offset,
                JOBS_INTEGRITY_BATCH_SIZE,
            )
            .await
            .map_err(|err| err.to_string())?;
            if rows.is_empty() {
                break;
            }
            offset += rows.len() as i64;
            let count = rows.len();
            let items: Vec<Value> = rows
                .into_iter()
                .map(|row| {
                    json!({
                        "reportId": row.report_id,
                        "path": row.path,
                        "checksum": row.checksum.map(|value| hex::encode(value)),
                    })
                })
                .collect();
            self.queue_job("IntegrityChecksumFilesRefresh", json!({ "items": items }))
                .await?;
            if count < JOBS_INTEGRITY_BATCH_SIZE as usize {
                break;
            }
        }
        Ok(())
    }

    async fn queue_report_refresh_batches(
        &self,
        report_type: &str,
        job_name: &str,
    ) -> Result<(), String> {
        let mut offset = 0i64;
        loop {
            let rows = integrity::stream_integrity_reports_with_checksum_page(
                &self.pool,
                report_type,
                offset,
                JOBS_INTEGRITY_BATCH_SIZE,
            )
            .await
            .map_err(|err| err.to_string())?;
            if rows.is_empty() {
                break;
            }
            offset += rows.len() as i64;
            let count = rows.len();
            let items: Vec<Value> = rows
                .into_iter()
                .map(|row| json!({ "reportId": row.report_id, "path": row.path }))
                .collect();
            self.queue_job(job_name, json!({ "items": items })).await?;
            if count < JOBS_INTEGRITY_BATCH_SIZE as usize {
                break;
            }
        }
        Ok(())
    }

    async fn queue_job(&self, name: &str, data: Value) -> Result<(), String> {
        self.jobs
            .queue_json_job(QUEUE_INTEGRITY, name, data)
            .await
            .map_err(|err| err.to_string())
    }
}

fn rows_to_missing_items(rows: Vec<AssetPathItemRow>) -> Vec<Value> {
    rows.into_iter()
        .map(|row| {
            json!({
                "path": row.path,
                "assetId": row.asset_id,
                "fileAssetId": row.file_asset_id,
                "reportId": row.report_id,
            })
        })
        .collect()
}

fn rows_to_delete_items(rows: Vec<IntegrityReportDeleteRow>) -> Vec<DeleteReportItem> {
    rows.into_iter()
        .map(|row| DeleteReportItem {
            id: row.id,
            path: row.path,
            asset_id: row.asset_id,
            file_asset_id: row.file_asset_id,
        })
        .collect()
}

async fn load_checksum_checkpoint(
    pool: &PgPool,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    let value = get_json(pool, CHECKSUM_CHECKPOINT_KEY).await?;
    Ok(value.and_then(|json| {
        json.get("date")
            .and_then(|date| date.as_str())
            .and_then(|date| DateTime::parse_from_rfc3339(date).ok())
            .map(|date| date.with_timezone(&Utc))
    }))
}

async fn save_checksum_checkpoint(
    pool: &PgPool,
    date: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    let value = json!({
        "date": date.map(|value| value.to_rfc3339()),
    });
    set_json(pool, CHECKSUM_CHECKPOINT_KEY, &value).await
}

pub fn spawn(pool: PgPool, redis_url: String, storage: StoragePaths, jobs: JobService, concurrency: usize) {
    tokio::spawn(async move {
        let processor = Arc::new(IntegrityProcessor::new(pool, storage, jobs));
        let worker = WorkerBuilder::new(QUEUE_INTEGRITY)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    let job_name = job.name.clone();
                    crate::service::workers::wrap_simple_job(QUEUE_INTEGRITY, &job_name, || async {
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
                eprintln!("integrityCheck worker failed to start: {err}");
            }
        }
    });
}
