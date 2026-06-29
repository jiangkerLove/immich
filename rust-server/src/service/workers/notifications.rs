use bullmq_rs::{WorkerBuilder, RedisConnection};
use serde_json::Value;
use sqlx::PgPool;

use crate::service::job::JobService;
use crate::service::notification::{NotificationJobProcessor, NotificationJobResult};
use crate::service::websocket::WebSocketHub;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_NOTIFICATION: &str = "notifications";

pub fn spawn(pool: PgPool, redis_url: String, websocket: WebSocketHub, jobs: JobService, concurrency: usize) {
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
                    crate::service::workers::begin_job(QUEUE_NOTIFICATION, &job_name);
                    let result = processor.process(&job_name, &job.data).await;
                    let success = matches!(
                        result,
                        Ok(NotificationJobResult::Success | NotificationJobResult::Skipped)
                    );
                    crate::service::workers::end_job(QUEUE_NOTIFICATION, &job_name, success);
                    match result {
                        Ok(NotificationJobResult::Success | NotificationJobResult::Skipped) => {
                            Ok(())
                        }
                        Err(err) => Err(crate::service::workers::worker_error(err.to_string())),
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
                eprintln!("notification job worker failed to start: {err}");
            }
        }
    });
}
