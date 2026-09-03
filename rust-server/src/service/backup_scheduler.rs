use std::time::Duration;

use chrono::Local;
use sqlx::PgPool;

use crate::models::db::advisory_lock::{self, LOCK_BACKUP_DATABASE};
use crate::service::job::JobService;
use crate::utils::cron::should_run_cron;
use crate::utils::system_config::get_merged;

const QUEUE_BACKUP: &str = "backupDatabase";

pub fn spawn(pool: PgPool, jobs: JobService) {
    tokio::spawn(async move {
        let lock = match advisory_lock::try_acquire(&pool, LOCK_BACKUP_DATABASE).await {
            Ok(Some(value)) => value,
            Ok(None) => {
                println!("backup scheduler: another instance holds the lock, skipping");
                return;
            }
            Err(err) => {
                eprintln!("backup scheduler: failed to acquire lock: {err}");
                return;
            }
        };
        let _lock = lock;

        println!("backup scheduler: started");
        let mut last_run: Option<chrono::DateTime<Local>> = None;

        loop {
            crate::service::scheduler_notify::wait_or_notify(Duration::from_secs(60)).await;

            let config = match get_merged(&pool).await {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("backup scheduler: failed to load config: {err}");
                    continue;
                }
            };

            let Some(database) = config.get("backup").and_then(|value| value.get("database"))
            else {
                continue;
            };

            let enabled = database
                .get("enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            if !enabled {
                continue;
            }

            let Some(expression) = database
                .get("cronExpression")
                .and_then(|value| value.as_str())
            else {
                continue;
            };

            let now = Local::now();
            let since = now - chrono::Duration::seconds(59);
            if !should_run_cron(expression, now, since) {
                continue;
            }

            if last_run.is_some_and(|previous| previous >= since) {
                continue;
            }

            if let Err(err) = jobs
                .queue_deduplicated_json_job(QUEUE_BACKUP, "DatabaseBackup", serde_json::json!({}))
                .await
            {
                eprintln!("backup scheduler: failed to queue DatabaseBackup: {err}");
            } else {
                println!("backup scheduler: queued DatabaseBackup");
                last_run = Some(now);
            }
        }
    });
}
