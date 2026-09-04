use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::dto::env::EnvDto;
use crate::service::job::JobService;
use crate::service::media::storage_template::{StorageTemplateOutcome, StorageTemplateService};
use crate::utils::storage::StoragePaths;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_STORAGE_TEMPLATE: &str = "storageTemplateMigration";

#[derive(Debug, Deserialize)]
struct EntityJobData {
    id: Uuid,
    source: Option<String>,
    notify: Option<bool>,
}

#[derive(Clone)]
pub struct StorageTemplateMigrationProcessor {
    service: StorageTemplateService,
}

impl StorageTemplateMigrationProcessor {
    pub fn new(pool: PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self {
            service: StorageTemplateService::new(pool, storage, jobs),
        }
    }

    async fn process(&self, name: &str, data: &Value) -> Result<JobWorkerStatus, String> {
        match name {
            "StorageTemplateMigrationSingle" => {
                let job: EntityJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                let entity_job = crate::service::job::EntityJob {
                    id: job.id,
                    source: job.source,
                    notify: job.notify,
                };
                match self.service.migrate_single(&job.id, &entity_job).await? {
                    StorageTemplateOutcome::Success => Ok(JobWorkerStatus::Success),
                    StorageTemplateOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    StorageTemplateOutcome::Failed => Ok(JobWorkerStatus::Failed),
                }
            }
            "StorageTemplateMigration" => match self.service.migrate_all().await? {
                StorageTemplateOutcome::Success => Ok(JobWorkerStatus::Success),
                StorageTemplateOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                StorageTemplateOutcome::Failed => Ok(JobWorkerStatus::Failed),
            },
            other => {
                tracing::warn!(
                    "storageTemplateMigration job {other} is not implemented in rust-server yet; skipping"
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
        let processor = Arc::new(StorageTemplateMigrationProcessor::new(pool, storage, jobs));

        let worker = WorkerBuilder::new(QUEUE_STORAGE_TEMPLATE)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    let job_name = job.name.clone();
                    crate::service::workers::wrap_status_job(
                        QUEUE_STORAGE_TEMPLATE,
                        &job_name,
                        || async {
                            processor
                                .process(&job_name, &job.data)
                                .await
                                .map(|status| status.as_str())
                        },
                    )
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
                tracing::error!("storageTemplateMigration worker failed to start: {err}");
            }
        }
    });
}
