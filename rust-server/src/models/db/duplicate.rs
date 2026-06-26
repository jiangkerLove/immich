use sqlx::{Pool, Postgres};
use uuid::Uuid;

pub async fn cleanup_singleton_groups(pool: &Pool<Postgres>, user_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            WITH singletons AS (
                SELECT "duplicateId"
                FROM asset
                WHERE "ownerId" = $1
                  AND "duplicateId" IS NOT NULL
                  AND "deletedAt" IS NULL
                  AND "stackId" IS NULL
                GROUP BY "duplicateId"
                HAVING COUNT(id) = 1
            )
            UPDATE asset
            SET "duplicateId" = NULL
            FROM singletons
            WHERE asset."duplicateId" = singletons."duplicateId"
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
pub struct DuplicateGroupRow {
    pub duplicate_id: Uuid,
}

pub async fn list_duplicate_ids(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
) -> Result<Vec<DuplicateGroupRow>, sqlx::Error> {
    sqlx::query_as::<_, DuplicateGroupRow>(
        r#"
            SELECT a."duplicateId" AS duplicate_id
            FROM asset a
            WHERE a."ownerId" = $1
              AND a."duplicateId" IS NOT NULL
              AND a."deletedAt" IS NULL
              AND a."stackId" IS NULL
              AND a.visibility IN ('archive', 'timeline')
            GROUP BY a."duplicateId"
            HAVING COUNT(*) > 1
            ORDER BY MIN(a."localDateTime") ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn list_asset_ids_by_duplicate_id(
    pool: &Pool<Postgres>,
    duplicate_id: &Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT id
            FROM asset
            WHERE "duplicateId" = $1
              AND "deletedAt" IS NULL
              AND "stackId" IS NULL
              AND visibility IN ('archive', 'timeline')
            ORDER BY "localDateTime" ASC
        "#,
    )
    .bind(duplicate_id)
    .fetch_all(pool)
    .await
}

pub async fn clear_duplicate_group(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    duplicate_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE asset
            SET "duplicateId" = NULL
            WHERE "ownerId" = $1 AND "duplicateId" = $2
        "#,
    )
    .bind(user_id)
    .bind(duplicate_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear_duplicate_groups(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    duplicate_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if duplicate_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"
            UPDATE asset
            SET "duplicateId" = NULL
            WHERE "ownerId" = $1 AND "duplicateId" = ANY($2)
        "#,
    )
    .bind(user_id)
    .bind(duplicate_ids)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn duplicate_group_exists(
    pool: &Pool<Postgres>,
    duplicate_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        r#"
            SELECT COUNT(*)
            FROM asset
            WHERE "duplicateId" = $1
              AND "deletedAt" IS NULL
              AND "stackId" IS NULL
        "#,
    )
    .bind(duplicate_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}
