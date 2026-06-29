use sqlx::PgPool;

use crate::service::cron_scheduler::{already_ran, mark_ran, spawn_locked};
use crate::service::job::JobService;
use crate::utils::cron::should_run_cron;
use crate::utils::system_config::get_merged;

const QUEUE_LIBRARY: &str = "library";
const SCHEDULER_NAME: &str = "library scheduler";
const JOB_ID: &str = "library-scan";

pub fn spawn(pool: PgPool, jobs: JobService) {
    let bootstrap_pool = pool.clone();
    let bootstrap_jobs = jobs.clone();
    spawn_locked(
        SCHEDULER_NAME,
        crate::models::db::advisory_lock::LOCK_LIBRARY,
        pool,
        jobs,
        Some(Box::pin(async move {
            crate::service::library_watcher::bootstrap(&bootstrap_pool, bootstrap_jobs).await;
        })),
        |pool, jobs, now, since, last_run| {
            Box::pin(async move {
                let config = match get_merged(&pool).await {
                    Ok(value) => value,
                    Err(err) => {
                        eprintln!("{SCHEDULER_NAME}: failed to load config: {err}");
                        return;
                    }
                };

                let Some(scan) = config.get("library").and_then(|value| value.get("scan")) else {
                    return;
                };

                let enabled = scan
                    .get("enabled")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true);
                if !enabled {
                    return;
                }

                let Some(expression) = scan
                    .get("cronExpression")
                    .and_then(|value| value.as_str())
                else {
                    return;
                };

                if !should_run_cron(expression, now, since) {
                    return;
                }

                if already_ran(&last_run, JOB_ID, since) {
                    return;
                }

                if let Err(err) = jobs
                    .queue_json_job_empty(QUEUE_LIBRARY, "LibraryScanQueueAll")
                    .await
                {
                    eprintln!("{SCHEDULER_NAME}: failed to queue LibraryScanQueueAll: {err}");
                } else {
                    println!("{SCHEDULER_NAME}: queued LibraryScanQueueAll");
                    mark_ran(&last_run, JOB_ID, now);
                }
            })
        },
    );
}
