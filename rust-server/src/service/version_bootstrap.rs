use sqlx::PgPool;

use crate::constants::SERVER_VERSION;
use crate::models::db::version_history;
use crate::service::job::JobService;

pub const LOCK_VERSION_HISTORY: i64 = 500;

const QUEUE_BACKGROUND: &str = "backgroundTask";
const MEMORY_REGEN_THRESHOLD: &str = "1.129.0";

pub async fn run(pool: &PgPool, jobs: &JobService) -> Result<(), String> {
    let mut lock_conn = pool.acquire().await.map_err(|err| err.to_string())?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(LOCK_VERSION_HISTORY)
        .execute(&mut *lock_conn)
        .await
        .map_err(|err| err.to_string())?;

    let result = sync_version_history(pool, jobs).await;

    let _: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(LOCK_VERSION_HISTORY)
        .fetch_one(&mut *lock_conn)
        .await
        .map_err(|err| err.to_string())?;

    result
}

async fn sync_version_history(pool: &PgPool, jobs: &JobService) -> Result<(), String> {
    let previous = version_history::get_latest_version(pool)
        .await
        .map_err(|err| err.to_string())?;
    let current = SERVER_VERSION;

    let Some(previous_version) = previous else {
        version_history::insert_version(pool, current)
            .await
            .map_err(|err| err.to_string())?;
        return Ok(());
    };

    if previous_version == current {
        return Ok(());
    }

    println!("version bootstrap: adding {current} to upgrade history");
    version_history::insert_version(pool, current)
        .await
        .map_err(|err| err.to_string())?;

    if should_regenerate_memories(&previous_version) {
        jobs
            .queue_json_job_empty(QUEUE_BACKGROUND, "MemoryGenerate")
            .await
            .map_err(|err| err.to_string())?;
        println!("version bootstrap: queued MemoryGenerate after upgrade from {previous_version}");
    }

    Ok(())
}

fn should_regenerate_memories(previous_version: &str) -> bool {
    match (
        semver::Version::parse(previous_version),
        semver::Version::parse(MEMORY_REGEN_THRESHOLD),
    ) {
        (Ok(previous), Ok(threshold)) => previous < threshold,
        _ => false,
    }
}
