use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::dto::env::EnvDto;
use crate::service::job::JobService;
use crate::service::media::smart_search::{SmartSearchOutcome, SmartSearchService};

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_SMART_SEARCH: &str = "smartSearch";

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
pub struct SmartSearchProcessor {
    service: SmartSearchService,
}

impl SmartSearchProcessor {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self {
            service: SmartSearchService::new(pool, jobs),
        }
    }

    async fn process(&self, name: &str, data: &Value) -> Result<JobWorkerStatus, String> {
        match name {
            "SmartSearch" => {
                let job: EntityJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                let entity_job = crate::service::job::EntityJob {
                    id: job.id,
                    source: job.source,
                    notify: job.notify,
                };
                match self.service.encode_asset(&job.id, &entity_job).await? {
                    SmartSearchOutcome::Success => Ok(JobWorkerStatus::Success),
                    SmartSearchOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    SmartSearchOutcome::Failed => Ok(JobWorkerStatus::Failed),
                }
            }
            "SmartSearchQueueAll" => {
                let job: QueueAllJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                self.service.queue_all(job.force.unwrap_or(false)).await?;
                Ok(JobWorkerStatus::Success)
            }
            other => {
                eprintln!("smartSearch job {other} is not implemented in rust-server yet; skipping");
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

pub fn spawn(pool: PgPool, redis_url: String, _env: EnvDto) {
    tokio::spawn(async move {
        let jobs = JobService::new(redis_url.clone());
        let processor = Arc::new(SmartSearchProcessor::new(pool, jobs));

        let worker = WorkerBuilder::new(QUEUE_SMART_SEARCH)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(2)
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
                eprintln!("smartSearch worker failed to start: {err}");
            }
        }
    });
}
