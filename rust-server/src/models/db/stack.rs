use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct StackRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub primary_asset_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub struct StackAssetRemovalRow {
    pub id: Option<Uuid>,
    pub primary_asset_id: Option<Uuid>,
}

pub async fn search(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    primary_asset_id: Option<&Uuid>,
) -> Result<Vec<StackRow>, sqlx::Error> {
    let mut query = String::from(
        r#"
            SELECT
                id,
                "ownerId" as owner_id,
                "primaryAssetId" as primary_asset_id,
                "createdAt" as created_at,
                "updatedAt" as updated_at
            FROM stack
            WHERE "ownerId" = $1
        "#,
    );
    if primary_asset_id.is_some() {
        query.push_str(r#" AND "primaryAssetId" = $2"#);
    }
    query.push_str(r#" ORDER BY "updatedAt" DESC"#);

    let mut q = sqlx::query_as::<_, StackRow>(&query).bind(owner_id);
    if let Some(primary_asset_id) = primary_asset_id {
        q = q.bind(primary_asset_id);
    }
    q.fetch_all(pool).await
}

pub async fn get_by_id(pool: &Pool<Postgres>, id: &Uuid) -> Result<Option<StackRow>, sqlx::Error> {
    sqlx::query_as::<_, StackRow>(
        r#"
            SELECT
                id,
                "ownerId" as owner_id,
                "primaryAssetId" as primary_asset_id,
                "createdAt" as created_at,
                "updatedAt" as updated_at
            FROM stack
            WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn list_asset_ids(pool: &Pool<Postgres>, stack_id: &Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT id
            FROM asset
            WHERE "stackId" = $1
              AND "deletedAt" IS NULL
              AND visibility != 'hidden'
            ORDER BY "fileCreatedAt" ASC
        "#,
    )
    .bind(stack_id)
    .fetch_all(pool)
    .await
}

pub async fn create(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<StackRow, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let existing_stack_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
            SELECT id
            FROM stack
            WHERE "ownerId" = $1
              AND "primaryAssetId" = ANY($2)
        "#,
    )
    .bind(owner_id)
    .bind(asset_ids)
    .fetch_all(&mut *tx)
    .await?;

    let mut unique_ids: Vec<Uuid> = asset_ids.to_vec();
    if !existing_stack_ids.is_empty() {
        let child_ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
                SELECT id
                FROM asset
                WHERE "stackId" = ANY($1)
                  AND "deletedAt" IS NULL
            "#,
        )
        .bind(&existing_stack_ids)
        .fetch_all(&mut *tx)
        .await?;

        for id in child_ids {
            if !unique_ids.contains(&id) {
                unique_ids.push(id);
            }
        }

        sqlx::query(r#"DELETE FROM stack WHERE id = ANY($1)"#)
            .bind(&existing_stack_ids)
            .execute(&mut *tx)
            .await?;
    }

    let stack_id: Uuid = sqlx::query_scalar(
        r#"
            INSERT INTO stack ("ownerId", "primaryAssetId")
            VALUES ($1, $2)
            RETURNING id
        "#,
    )
    .bind(owner_id)
    .bind(asset_ids.first())
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"
            UPDATE asset
            SET "stackId" = $1, "updatedAt" = NOW()
            WHERE id = ANY($2)
        "#,
    )
    .bind(stack_id)
    .bind(&unique_ids)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    get_by_id(pool, &stack_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn update_primary(
    pool: &Pool<Postgres>,
    stack_id: &Uuid,
    primary_asset_id: &Uuid,
) -> Result<StackRow, sqlx::Error> {
    sqlx::query_as::<_, StackRow>(
        r#"
            UPDATE stack
            SET "primaryAssetId" = $2
            WHERE id = $1
            RETURNING
                id,
                "ownerId" as owner_id,
                "primaryAssetId" as primary_asset_id,
                "createdAt" as created_at,
                "updatedAt" as updated_at
        "#,
    )
    .bind(stack_id)
    .bind(primary_asset_id)
    .fetch_one(pool)
    .await
}

pub async fn delete(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM stack WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_all(pool: &Pool<Postgres>, ids: &[Uuid]) -> Result<(), sqlx::Error> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query(r#"DELETE FROM stack WHERE id = ANY($1)"#)
        .bind(ids)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_for_asset_removal(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<StackAssetRemovalRow>, sqlx::Error> {
    sqlx::query_as::<_, StackAssetRemovalRow>(
        r#"
            SELECT
                a."stackId" as id,
                s."primaryAssetId" as primary_asset_id
            FROM asset a
            LEFT JOIN stack s ON s.id = a."stackId"
            WHERE a.id = $1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn remove_asset_from_stack(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE asset
            SET "stackId" = NULL, "updatedAt" = NOW()
            WHERE id = $1
        "#,
    )
    .bind(asset_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn owner_owns_stacks(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    stack_ids: &[Uuid],
) -> Result<bool, sqlx::Error> {
    if stack_ids.is_empty() {
        return Ok(true);
    }
    let count: i64 = sqlx::query_scalar(
        r#"
            SELECT COUNT(*)
            FROM stack
            WHERE id = ANY($1) AND "ownerId" = $2
        "#,
    )
    .bind(stack_ids)
    .bind(owner_id)
    .fetch_one(pool)
    .await?;
    Ok(count as usize == stack_ids.len())
}

pub async fn merge_stacks(
    pool: &Pool<Postgres>,
    source_stack_id: &Uuid,
    target_stack_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE asset
            SET "stackId" = $2, "updatedAt" = NOW()
            WHERE "stackId" = $1
        "#,
    )
    .bind(source_stack_id)
    .bind(target_stack_id)
    .execute(pool)
    .await?;
    Ok(())
}
