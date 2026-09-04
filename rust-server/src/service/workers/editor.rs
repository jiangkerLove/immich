use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::dto::env::EnvDto;
use crate::service::job::JobService;
use crate::service::media::thumbnail::{ThumbnailJobOutcome, ThumbnailService};
use crate::utils::storage::StoragePaths;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_EDITOR: &str = "editor";

#[derive(Debug, Deserialize)]
struct EntityJobData {
    id: Uuid,
}

#[derive(Clone)]
pub struct EditorProcessor {
    service: ThumbnailService,
}

impl EditorProcessor {
    pub fn new(pool: PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self {
            service: ThumbnailService::new(pool, storage, jobs),
        }
    }

    pub async fn process(&self, name: &str, data: &Value) -> Result<JobWorkerStatus, String> {
        match name {
            "AssetEditThumbnailGeneration" => {
                let job: EntityJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                match self.service.generate_asset_edit_thumbnails(&job.id).await? {
                    ThumbnailJobOutcome::Success => Ok(JobWorkerStatus::Success),
                    ThumbnailJobOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    ThumbnailJobOutcome::Failed => Ok(JobWorkerStatus::Failed),
                }
            }
            other => {
                tracing::warn!("editor job {other} is not implemented in rust-server yet; skipping");
                Ok(JobWorkerStatus::Skipped)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobWorkerStatus {
    Success,
    Skipped,
    Failed,
}

impl JobWorkerStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

pub fn spawn(
    pool: PgPool,
    redis_url: String,
    storage: StoragePaths,
    _env: EnvDto,
    concurrency: usize,
) {
    tokio::spawn(async move {
        let jobs = JobService::new(redis_url.clone());
        let processor = Arc::new(EditorProcessor::new(pool, storage, jobs));

        let worker = WorkerBuilder::new(QUEUE_EDITOR)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    let job_name = job.name.clone();
                    crate::service::workers::wrap_status_job(QUEUE_EDITOR, &job_name, || async {
                        processor
                            .process(&job_name, &job.data)
                            .await
                            .map(|status| status.as_str())
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
                tracing::error!("editor worker failed to start: {err}");
            }
        }
    });
}
