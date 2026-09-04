use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde_json::Value;
use sqlx::PgPool;

use crate::service::job::JobService;
use crate::service::notification::{NotificationJobProcessor, NotificationJobResult};
use crate::service::websocket::WebSocketHub;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_NOTIFICATION: &str = "notifications";

pub fn spawn(
    pool: PgPool,
    redis_url: String,
    websocket: WebSocketHub,
    jobs: JobService,
    concurrency: usize,
) {
    tokio::spawn(async move {
        let processor = NotificationJobProcessor::with_jobs(pool, websocket, jobs);
        let worker = WorkerBuilder::new(QUEUE_NOTIFICATION)
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
                        QUEUE_NOTIFICATION,
                        &job_name,
                        || async {
                            processor
                                .process(&job_name, &job.data)
                                .await
                                .map(|result| match result {
                                    NotificationJobResult::Success => "success",
                                    NotificationJobResult::Skipped => "skipped",
                                })
                                .map_err(|err| err.to_string())
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
                tracing::error!("notification job worker failed to start: {err}");
            }
        }
    });
}
