use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde_json::Value;
use sqlx::PgPool;

use crate::models::dto::env::EnvDto;
use crate::service::database_backup_runner::DatabaseBackupRunner;
use crate::utils::storage::StoragePaths;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_BACKUP: &str = "backupDatabase";

#[derive(Clone)]
pub struct BackupDatabaseProcessor {
    runner: DatabaseBackupRunner,
}

impl BackupDatabaseProcessor {
    pub fn new(pool: PgPool, storage: StoragePaths, env: EnvDto) -> Self {
        Self {
            runner: DatabaseBackupRunner::new(pool, storage, env),
        }
    }

    pub async fn process(&self, name: &str) -> Result<(), String> {
        match name {
            "DatabaseBackup" => self
                .runner
                .run_backup()
                .await
                .map_err(|err| err.to_string()),
            other => Err(format!("unknown backupDatabase job: {other}")),
        }
    }
}

pub fn spawn(pool: PgPool, redis_url: String, storage: StoragePaths, env: EnvDto, concurrency: usize) {
    tokio::spawn(async move {
        let processor = Arc::new(BackupDatabaseProcessor::new(pool, storage, env));
        let worker = WorkerBuilder::new(QUEUE_BACKUP)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    let job_name = job.name.clone();
                    crate::service::workers::begin_job(QUEUE_BACKUP, &job_name);
                    let result = match processor.process(&job_name).await {
                        Ok(()) => Ok(()),
                        Err(err) if matches_unsupported_postgres(&err) => {
                            eprintln!("database backup skipped: {err}");
                            Ok(())
                        }
                        Err(err) => Err(err),
                    };
                    crate::service::workers::end_job(QUEUE_BACKUP, &job_name, result.is_ok());
                    result.map_err(crate::service::workers::worker_error)
                }
            })
            .await;

        match handle {
            Ok(worker_handle) => {
                crate::service::worker_registry::register(worker_handle);
                std::future::pending::<()>().await;
            }
            Err(err) => {
                eprintln!("backupDatabase worker failed to start: {err}");
            }
        }
    });
}

fn matches_unsupported_postgres(message: &str) -> bool {
    message.contains("unsupported PostgreSQL version")
        || message.contains("UnsupportedPostgres")
}
