use bullmq_rs::{WorkerBuilder, RedisConnection};
use serde_json::Value;
use sqlx::PgPool;

use crate::service::job::JobService;
use crate::service::notification::{NotificationJobProcessor, NotificationJobResult};
use crate::service::websocket::WebSocketHub;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_NOTIFICATION: &str = "notifications";

pub fn spawn(pool: PgPool, redis_url: String, websocket: WebSocketHub, jobs: JobService) {
    tokio::spawn(async move {
        let processor = NotificationJobProcessor::with_jobs(pool, websocket, jobs);
        let worker = WorkerBuilder::new(QUEUE_NOTIFICATION)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(5)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    match processor.process(&job.name, &job.data).await {
                        Ok(NotificationJobResult::Success) | Ok(NotificationJobResult::Skipped) => {
                            Ok(())
                        }
                        Err(err) => Err(Box::new(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            err.to_string(),
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
                eprintln!("notification job worker failed to start: {err}");
            }
        }
    });
}
