use sqlx::pool::PoolConnection;
use sqlx::{Pool, Postgres};

pub const LOCK_INTEGRITY_CHECK: i64 = 67;
pub const LOCK_BACKUP_DATABASE: i64 = 42;
pub const LOCK_LIBRARY: i64 = 1337;
pub const LOCK_NIGHTLY_JOBS: i64 = 600;
pub const LOCK_VERSION_CHECK: i64 = 800;
pub const LOCK_MEMORY_CREATION: i64 = 777;

pub async fn try_acquire(
    pool: &Pool<Postgres>,
    lock_id: i64,
) -> Result<Option<PoolConnection<Postgres>>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(lock_id)
        .fetch_one(&mut *conn)
        .await?;

    if acquired {
        Ok(Some(conn))
    } else {
        Ok(None)
    }
}

pub async fn run_with_try_lock<F, Fut, T>(
    pool: &Pool<Postgres>,
    lock_id: i64,
    f: F,
) -> Result<Option<T>, sqlx::Error>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
{
    let mut conn = match try_acquire(pool, lock_id).await? {
        Some(value) => value,
        None => return Ok(None),
    };

    let result = f().await;
    let _: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(lock_id)
        .fetch_one(&mut *conn)
        .await?;
    result.map(Some)
}
