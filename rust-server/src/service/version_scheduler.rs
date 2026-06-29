use sqlx::PgPool;

use crate::service::cron_scheduler::{already_ran, mark_ran, spawn_locked};
use crate::service::job::JobService;
use crate::utils::cron::should_run_cron;

const QUEUE_BACKGROUND: &str = "backgroundTask";
const SCHEDULER_NAME: &str = "version scheduler";
const JOB_ID: &str = "version-check";

pub fn spawn(pool: PgPool, jobs: JobService) {
    let random_minute = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
        % 60) as u32;
    let cron_expression = format!("0 {random_minute} * * * *");

    let bootstrap_jobs = jobs.clone();
    let bootstrap = Box::pin(async move {
        if let Err(err) = bootstrap_jobs
            .queue_json_job_empty(QUEUE_BACKGROUND, "VersionCheck")
            .await
        {
            eprintln!("{SCHEDULER_NAME}: failed to queue initial VersionCheck: {err}");
        } else {
            println!("{SCHEDULER_NAME}: queued initial VersionCheck");
        }
    });

    spawn_locked(
        SCHEDULER_NAME,
        crate::models::db::advisory_lock::LOCK_VERSION_CHECK,
        pool,
        jobs,
        Some(bootstrap),
        move |_pool, jobs, now, since, last_run| {
            let cron_expression = cron_expression.clone();
            Box::pin(async move {
                if !should_run_cron(&cron_expression, now, since) {
                    return;
                }

                if already_ran(&last_run, JOB_ID, since) {
                    return;
                }

                if let Err(err) = jobs
                    .queue_json_job_empty(QUEUE_BACKGROUND, "VersionCheck")
                    .await
                {
                    eprintln!("{SCHEDULER_NAME}: failed to queue VersionCheck: {err}");
                } else {
                    println!("{SCHEDULER_NAME}: queued VersionCheck");
                    mark_ran(&last_run, JOB_ID, now);
                }
            })
        },
    );
}
