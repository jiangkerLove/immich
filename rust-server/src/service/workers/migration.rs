use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::dto::env::EnvDto;
use crate::service::job::{JobService, PersonJob};
use crate::service::media::file_migration::{FileMigrationOutcome, FileMigrationService};
use crate::utils::storage::StoragePaths;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_MIGRATION: &str = "migration";

#[derive(Debug, Deserialize)]
struct EntityJobData {
    id: Uuid,
}

#[derive(Clone)]
pub struct MigrationProcessor {
    service: FileMigrationService,
}

impl MigrationProcessor {
    pub fn new(pool: PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self {
            service: FileMigrationService::new(pool, storage, jobs),
        }
    }

    async fn process(&self, name: &str, data: &Value) -> Result<JobWorkerStatus, String> {
        match name {
            "FileMigrationQueueAll" => {
                self.service.queue_all().await?;
                Ok(JobWorkerStatus::Success)
            }
            "AssetFileMigration" => {
                let job: EntityJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                match self.service.migrate_asset(&job.id).await? {
                    FileMigrationOutcome::Success => Ok(JobWorkerStatus::Success),
                    FileMigrationOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    FileMigrationOutcome::Failed => Ok(JobWorkerStatus::Failed),
                }
            }
            "PersonFileMigration" => {
                let job: PersonJob =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                match self
                    .service
                    .migrate_person(&job.owner_id, &job.person_group_id)
                    .await?
                {
                    FileMigrationOutcome::Success => Ok(JobWorkerStatus::Success),
                    FileMigrationOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    FileMigrationOutcome::Failed => Ok(JobWorkerStatus::Failed),
                }
            }
            other => {
                eprintln!("migration job {other} is not implemented in rust-server yet; skipping");
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
        let processor = Arc::new(MigrationProcessor::new(pool, storage, jobs));

        let worker = WorkerBuilder::new(QUEUE_MIGRATION)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    let job_name = job.name.clone();
                    crate::service::workers::wrap_status_job(QUEUE_MIGRATION, &job_name, || async {
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
                eprintln!("migration worker failed to start: {err}");
            }
        }
    });
}
