use sqlx::PgPool;

use crate::service::cron_scheduler::{already_ran, mark_ran, spawn_locked};
use crate::service::job::JobService;
use crate::utils::cron::{nightly_tasks_cron_expression, should_run_cron};
use crate::utils::system_config::get_merged;

const SCHEDULER_NAME: &str = "nightly scheduler";
const JOB_ID: &str = "nightly";

pub fn spawn(pool: PgPool, jobs: JobService) {
    spawn_locked(
        SCHEDULER_NAME,
        crate::models::db::advisory_lock::LOCK_NIGHTLY_JOBS,
        pool,
        jobs,
        None,
        |pool, jobs, now, since, last_run| {
            Box::pin(async move {
                let config = match get_merged(&pool).await {
                    Ok(value) => value,
                    Err(err) => {
                        tracing::error!("{SCHEDULER_NAME}: failed to load config: {err}");
                        return;
                    }
                };

                let expression = nightly_tasks_cron_expression(&config);
                if !should_run_cron(&expression, now, since) {
                    return;
                }

                if already_ran(&last_run, JOB_ID, since) {
                    return;
                }

                if let Err(err) = jobs.queue_nightly_jobs(&config).await {
                    tracing::error!("{SCHEDULER_NAME}: failed to queue jobs: {err}");
                } else {
                    tracing::info!("{SCHEDULER_NAME}: queued nightly jobs");
                    mark_ran(&last_run, JOB_ID, now);
                }
            })
        },
    );
}
