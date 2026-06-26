use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_BACKGROUND: &str = "backgroundTask";

#[derive(Debug, Deserialize)]
struct FileDeleteJob {
    files: Vec<String>,
}

#[derive(Clone)]
pub struct BackgroundTaskProcessor {
    pool: PgPool,
}

impl BackgroundTaskProcessor {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn process(&self, name: &str, data: &Value) -> Result<(), String> {
        match name {
            "FileDelete" => self.handle_file_delete(data).await,
            "SessionCleanup" => self.handle_session_cleanup().await,
            "NotificationsCleanup" => self.handle_notifications_cleanup().await,
            other => {
                eprintln!(
                    "backgroundTask job {other} is not implemented in rust-server yet; skipping"
                );
                Ok(())
            }
        }
    }

    async fn handle_file_delete(&self, data: &Value) -> Result<(), String> {
        let job: FileDeleteJob =
            serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
        for file in job.files {
            if file.is_empty() {
                continue;
            }
            if let Err(err) = tokio::fs::remove_file(&file).await {
                eprintln!("unable to remove file from disk ({file}): {err}");
            }
        }
        Ok(())
    }

    async fn handle_session_cleanup(&self) -> Result<(), String> {
        let deleted = crate::models::db::sessions::SessionPO::cleanup_expired(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if deleted > 0 {
            println!("deleted {deleted} expired session token(s)");
        }
        Ok(())
    }

    async fn handle_notifications_cleanup(&self) -> Result<(), String> {
        crate::models::db::notification::cleanup_old(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }
}

pub fn spawn(pool: PgPool, redis_url: String) {
    tokio::spawn(async move {
        let processor = Arc::new(BackgroundTaskProcessor::new(pool));
        let worker = WorkerBuilder::new(QUEUE_BACKGROUND)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(5)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    processor
                        .process(&job.name, &job.data)
                        .await
                        .map_err(|err| {
                            Box::new(std::io::Error::new(std::io::ErrorKind::Other, err))
                                as Box<dyn std::error::Error + Send + Sync>
                        })
                }
            })
            .await;

        match handle {
            Ok(_handle) => {
                std::future::pending::<()>().await;
            }
            Err(err) => {
                eprintln!("backgroundTask worker failed to start: {err}");
            }
        }
    });
}
