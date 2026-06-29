use sqlx::{Pool, Postgres};
use uuid::Uuid;

pub async fn restore_all_for_user(pool: &Pool<Postgres>, user_id: &Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
            UPDATE asset
            SET status = 'active', "deletedAt" = NULL
            WHERE "ownerId" = $1 AND status = 'trashed'
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn empty_for_user(pool: &Pool<Postgres>, user_id: &Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
            UPDATE asset
            SET status = 'deleted'
            WHERE "ownerId" = $1 AND status = 'trashed'
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn restore_by_ids(pool: &Pool<Postgres>, asset_ids: &[Uuid]) -> Result<u64, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(0);
    }

    let result = sqlx::query(
        r#"
            UPDATE asset
            SET status = 'active', "deletedAt" = NULL
            WHERE status = 'trashed' AND id = ANY($1)
        "#,
    )
    .bind(asset_ids)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn list_deleted_ids(pool: &Pool<Postgres>) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT id
            FROM asset
            WHERE status = 'deleted'
        "#,
    )
    .fetch_all(pool)
    .await
}
