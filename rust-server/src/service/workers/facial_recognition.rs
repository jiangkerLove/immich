use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::dto::env::EnvDto;
use crate::service::job::JobService;
use crate::service::media::facial_recognition::{
    FacialRecognitionOutcome, FacialRecognitionQueueAllOutcome, FacialRecognitionService,
};

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_FACIAL: &str = "facialRecognition";

#[derive(Debug, Deserialize)]
struct FacialRecognitionJobData {
    id: Uuid,
    deferred: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct QueueAllJobData {
    force: Option<bool>,
    nightly: Option<bool>,
    #[serde(rename = "clusterGroupId")]
    cluster_group_id: Option<Uuid>,
}

#[derive(Clone)]
pub struct FacialRecognitionProcessor {
    service: FacialRecognitionService,
}

impl FacialRecognitionProcessor {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self {
            service: FacialRecognitionService::new(pool, jobs),
        }
    }

    async fn process(&self, name: &str, data: &Value) -> Result<JobWorkerStatus, String> {
        match name {
            "FacialRecognition" => {
                let job: FacialRecognitionJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                match self
                    .service
                    .recognize_face(&job.id, job.deferred.unwrap_or(false))
                    .await?
                {
                    FacialRecognitionOutcome::Success => Ok(JobWorkerStatus::Success),
                    FacialRecognitionOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    FacialRecognitionOutcome::Failed => Ok(JobWorkerStatus::Failed),
                }
            }
            "FacialRecognitionQueueAll" => {
                let job: QueueAllJobData =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                match self
                    .service
                    .queue_all(
                        job.force.unwrap_or(false),
                        job.nightly.unwrap_or(false),
                        job.cluster_group_id,
                    )
                    .await?
                {
                    FacialRecognitionQueueAllOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    FacialRecognitionQueueAllOutcome::Success => Ok(JobWorkerStatus::Success),
                }
            }
            other => {
                eprintln!(
                    "facialRecognition job {other} is not implemented in rust-server yet; skipping"
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
        let processor = Arc::new(FacialRecognitionProcessor::new(pool, jobs));

        let worker = WorkerBuilder::new(QUEUE_FACIAL)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    let job_name = job.name.clone();
                    crate::service::workers::wrap_status_job(QUEUE_FACIAL, &job_name, || async {
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
                eprintln!("facialRecognition worker failed to start: {err}");
            }
        }
    });
}
