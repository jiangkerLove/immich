use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::dto::env::EnvDto;
use crate::service::job::JobService;
use crate::service::media::ocr::{OcrOutcome, OcrQueueAllOutcome, OcrService};

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_OCR: &str = "ocr";

#[derive(Debug, Deserialize)]
struct EntityJobData {
    id: Uuid,
}

#[derive(Debug, Deserialize)]
struct QueueAllJobData {
    force: Option<bool>,
}

#[derive(Clone)]
pub struct OcrProcessor {
    service: OcrService,
}

impl OcrProcessor {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self {
            service: OcrService::new(pool, jobs),
        }
    }

    async fn process(&self, name: &str, data: &Value) -> Result<JobWorkerStatus, String> {
        match name {
            "Ocr" => {
                let job: EntityJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                match self.service.process_asset(&job.id).await? {
                    OcrOutcome::Success => Ok(JobWorkerStatus::Success),
                    OcrOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    OcrOutcome::Failed => Ok(JobWorkerStatus::Failed),
                }
            }
            "OcrQueueAll" => {
                let job: QueueAllJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                match self.service.queue_all(job.force.unwrap_or(false)).await? {
                    OcrQueueAllOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    OcrQueueAllOutcome::Success => Ok(JobWorkerStatus::Success),
                }
            }
            other => {
                eprintln!("ocr job {other} is not implemented in rust-server yet; skipping");
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

pub fn spawn(pool: PgPool, redis_url: String, _env: EnvDto, concurrency: usize) {
    tokio::spawn(async move {
        let jobs = JobService::new(redis_url.clone());
        let processor = Arc::new(OcrProcessor::new(pool, jobs));

        let worker = WorkerBuilder::new(QUEUE_OCR)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    let job_name = job.name.clone();
                    crate::service::workers::wrap_status_job(QUEUE_OCR, &job_name, || async {
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
                eprintln!("ocr worker failed to start: {err}");
            }
        }
    });
}
