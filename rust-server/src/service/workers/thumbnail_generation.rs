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
                let job: EntityJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                match self.service.generate_person_thumbnail(&job.id).await? {
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
                eprintln!(
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

pub fn spawn(pool: PgPool, redis_url: String, storage: StoragePaths, _env: EnvDto) {
    tokio::spawn(async move {
        let jobs = JobService::new(redis_url.clone());
        let processor = Arc::new(ThumbnailGenerationProcessor::new(
            pool,
            storage,
            jobs,
        ));

        let worker = WorkerBuilder::new(QUEUE_THUMBNAIL)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(3)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    match processor.process(&job.name, &job.data).await {
                        Ok(status) if status == JobWorkerStatus::Failed => {
                            Err(Box::new(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                status.as_str(),
                            ))
                                as Box<dyn std::error::Error + Send + Sync>)
                        }
                        Ok(_) => Ok(()),
                        Err(err) => Err(Box::new(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            err,
                        ))
                            as Box<dyn std::error::Error + Send + Sync>),
                    }
                }
            })
            .await;

        match handle {
            Ok(_handle) => {
                std::future::pending::<()>().await;
            }
            Err(err) => {
                eprintln!("thumbnailGeneration worker failed to start: {err}");
            }
        }
    });
}
