use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::dto::env::EnvDto;
use crate::service::job::{JobService, PersonJob};
use crate::service::media::thumbnail::{ThumbnailJobOutcome, ThumbnailService};
use crate::utils::storage::StoragePaths;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_THUMBNAIL: &str = "thumbnailGeneration";

#[derive(Debug, Deserialize)]
struct EntityJobData {
    id: Uuid,
    source: Option<String>,
    notify: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct QueueAllJobData {
    force: Option<bool>,
}

#[derive(Clone)]
pub struct ThumbnailGenerationProcessor {
    service: ThumbnailService,
}

impl ThumbnailGenerationProcessor {
    pub fn new(pool: PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self {
            service: ThumbnailService::new(pool, storage, jobs),
        }
    }

    pub async fn process(&self, name: &str, data: &Value) -> Result<JobWorkerStatus, String> {
        match name {
            "AssetGenerateThumbnails" => {
                let job: EntityJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                let entity_job = crate::service::job::EntityJob {
                    id: job.id,
                    source: job.source,
                    notify: job.notify,
                };
                match self
                    .service
                    .generate_asset_thumbnails(&job.id, &entity_job)
                    .await?
                {
                    ThumbnailJobOutcome::Success => Ok(JobWorkerStatus::Success),
                    ThumbnailJobOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    ThumbnailJobOutcome::Failed => Ok(JobWorkerStatus::Failed),
                }
            }
            "PersonGenerateThumbnail" => {
                let job: PersonJob =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                match self
                    .service
                    .generate_person_thumbnail(&job.owner_id, &job.person_group_id)
                    .await?
                {
                    ThumbnailJobOutcome::Success => Ok(JobWorkerStatus::Success),
                    ThumbnailJobOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    ThumbnailJobOutcome::Failed => Ok(JobWorkerStatus::Failed),
                }
            }
            "AssetGenerateThumbnailsQueueAll" => {
                let job: QueueAllJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                self.service
                    .queue_all_thumbnails(job.force.unwrap_or(false))
                    .await?;
                Ok(JobWorkerStatus::Success)
            }
            other => {
                tracing::warn!(
                    "thumbnailGeneration job {other} is not implemented in rust-server yet; skipping"
                );
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
        let processor = Arc::new(ThumbnailGenerationProcessor::new(pool, storage, jobs));

        let worker = WorkerBuilder::new(QUEUE_THUMBNAIL)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    let job_name = job.name.clone();
                    crate::service::workers::wrap_status_job(QUEUE_THUMBNAIL, &job_name, || async {
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
                tracing::error!("thumbnailGeneration worker failed to start: {err}");
            }
        }
    });
}
