use std::collections::HashMap;
use std::time::Duration;

use chrono::Local;
use sqlx::PgPool;

use crate::models::db::advisory_lock::{self, LOCK_INTEGRITY_CHECK};
use crate::service::job::JobService;
use crate::utils::cron::should_run_cron;
use crate::utils::system_config::get_merged;

const QUEUE_INTEGRITY: &str = "integrityCheck";

struct IntegrityCronJob {
    config_key: &'static str,
    job_name: &'static str,
}

const INTEGRITY_JOBS: [IntegrityCronJob; 3] = [
    IntegrityCronJob {
        config_key: "untrackedFiles",
        job_name: "IntegrityUntrackedFilesQueueAll",
    },
    IntegrityCronJob {
        config_key: "missingFiles",
        job_name: "IntegrityMissingFilesQueueAll",
    },
    IntegrityCronJob {
        config_key: "checksumFiles",
        job_name: "IntegrityChecksumFiles",
    },
];

pub fn spawn(pool: PgPool, jobs: JobService) {
    tokio::spawn(async move {
        let lock = match advisory_lock::try_acquire(&pool, LOCK_INTEGRITY_CHECK).await {
            Ok(Some(value)) => value,
            Ok(None) => {
                tracing::info!("integrity scheduler: another instance holds the lock, skipping");
                return;
            }
            Err(err) => {
                tracing::error!("integrity scheduler: failed to acquire lock: {err}");
                return;
            }
        };
        let _lock = lock;

        tracing::info!("integrity scheduler: started");
        let mut last_run: HashMap<&'static str, chrono::DateTime<Local>> = HashMap::new();

        loop {
            crate::service::scheduler_notify::wait_or_notify(Duration::from_secs(60)).await;

            let config = match get_merged(&pool).await {
                Ok(value) => value,
                Err(err) => {
                    tracing::error!("integrity scheduler: failed to load config: {err}");
                    continue;
                }
            };

            let Some(checks) = config.get("integrityChecks") else {
                continue;
            };

            let now = Local::now();
            let since = now - chrono::Duration::seconds(59);

            for job in INTEGRITY_JOBS {
                let Some(entry) = checks.get(job.config_key) else {
                    continue;
                };

                let enabled = entry
                    .get("enabled")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true);
                if !enabled {
                    continue;
                }

                let Some(expression) = entry
                    .get("cronExpression")
                    .and_then(|value| value.as_str())
                else {
                    continue;
                };

                if !should_run_cron(expression, now, since) {
                    continue;
                }

                if last_run
                    .get(job.config_key)
                    .is_some_and(|previous| *previous >= since)
                {
                    continue;
                }

                if let Err(err) = jobs
                    .queue_json_job_empty(QUEUE_INTEGRITY, job.job_name)
                    .await
                {
                    tracing::error!(
                        "integrity scheduler: failed to queue {}: {err}",
                        job.job_name
                    );
                } else {
                    tracing::info!("integrity scheduler: queued {}", job.job_name);
                    last_run.insert(job.config_key, now);
                }
            }
        }
    });
}
