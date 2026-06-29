use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::dto::env::EnvDto;
use crate::service::job::JobService;
use crate::service::media::face_detection::{FaceDetectionOutcome, FaceDetectionService};

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_FACE: &str = "faceDetection";

#[derive(Debug, Deserialize)]
struct EntityJobData {
    id: Uuid,
}

#[derive(Debug, Deserialize)]
struct QueueAllJobData {
    force: Option<bool>,
}

#[derive(Clone)]
pub struct FaceDetectionProcessor {
    service: FaceDetectionService,
}

impl FaceDetectionProcessor {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self {
            service: FaceDetectionService::new(pool, jobs),
        }
    }

    async fn process(&self, name: &str, data: &Value) -> Result<JobWorkerStatus, String> {
        match name {
            "AssetDetectFaces" => {
                let job: EntityJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                match self.service.detect_asset(&job.id).await? {
                    FaceDetectionOutcome::Success => Ok(JobWorkerStatus::Success),
                    FaceDetectionOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    FaceDetectionOutcome::Failed => Ok(JobWorkerStatus::Failed),
                }
            }
            "AssetDetectFacesQueueAll" => {
                let job: QueueAllJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                self.service.queue_all(job.force).await?;
                Ok(JobWorkerStatus::Success)
            }
            other => {
                eprintln!(
                    "faceDetection job {other} is not implemented in rust-server yet; skipping"
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

pub fn spawn(pool: PgPool, redis_url: String, _env: EnvDto, concurrency: usize) {
    tokio::spawn(async move {
        let jobs = JobService::new(redis_url.clone());
        let processor = Arc::new(FaceDetectionProcessor::new(pool, jobs));

        let worker = WorkerBuilder::new(QUEUE_FACE)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    {
                        let job_name = job.name.clone();
                        crate::service::workers::begin_job(QUEUE_FACE, &job_name);
                        let result = processor.process(&job_name, &job.data).await;
                        let failed = matches!(result, Ok(JobWorkerStatus::Failed) | Err(_));
                        crate::service::workers::end_job(QUEUE_FACE, &job_name, !failed);
                        match result {
                            Ok(status) if status == JobWorkerStatus::Failed => {
                                Err(crate::service::workers::worker_error(status.as_str()))
                            }
                            Ok(_) => Ok(()),
                            Err(err) => Err(crate::service::workers::worker_error(err)),
                        }
                    }
                }
            })
            .await;

        match handle {
            Ok(worker_handle) => {
                crate::service::worker_registry::register(worker_handle);
                std::future::pending::<()>().await;
            }
            Err(err) => {
                eprintln!("faceDetection worker failed to start: {err}");
            }
        }
    });
}
