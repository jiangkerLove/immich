use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct AssetMetadataRow {
    pub key: String,
    pub value: Value,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct AssetMetadataBulkRow {
    pub asset_id: Uuid,
    pub key: String,
    pub value: Value,
    pub updated_at: DateTime<Utc>,
}

pub async fn list_by_asset(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Vec<AssetMetadataRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetMetadataRow>(
        r#"
            SELECT key, value, "updatedAt" AS updated_at
            FROM asset_metadata
            WHERE "assetId" = $1
            ORDER BY key ASC
        "#,
    )
    .bind(asset_id)
    .fetch_all(pool)
    .await
}

pub async fn get_by_key(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    key: &str,
) -> Result<Option<AssetMetadataRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetMetadataRow>(
        r#"
            SELECT key, value, "updatedAt" AS updated_at
            FROM asset_metadata
            WHERE "assetId" = $1 AND key = $2
        "#,
    )
    .bind(asset_id)
    .bind(key)
    .fetch_optional(pool)
    .await
}

pub async fn upsert_items(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    items: &[(String, Value)],
) -> Result<Vec<AssetMetadataRow>, sqlx::Error> {
    if items.is_empty() {
        return Ok(vec![]);
    }

    let mut results = Vec::with_capacity(items.len());
    for (key, value) in items {
        let row = sqlx::query_as::<_, AssetMetadataRow>(
            r#"
                INSERT INTO asset_metadata ("assetId", key, value)
                VALUES ($1, $2, $3)
                ON CONFLICT ("assetId", key) DO UPDATE SET value = EXCLUDED.value
                RETURNING key, value, "updatedAt" AS updated_at
            "#,
        )
        .bind(asset_id)
        .bind(key)
        .bind(value)
        .fetch_one(pool)
        .await?;
        results.push(row);
    }
    Ok(results)
}

pub async fn upsert_bulk(
    pool: &Pool<Postgres>,
    items: &[(Uuid, String, Value)],
) -> Result<Vec<AssetMetadataBulkRow>, sqlx::Error> {
    if items.is_empty() {
        return Ok(vec![]);
    }

    let mut results = Vec::with_capacity(items.len());
    for (asset_id, key, value) in items {
        let row = sqlx::query_as::<_, AssetMetadataBulkRow>(
            r#"
                INSERT INTO asset_metadata ("assetId", key, value)
                VALUES ($1, $2, $3)
                ON CONFLICT ("assetId", key) DO UPDATE SET value = EXCLUDED.value
                RETURNING "assetId" AS asset_id, key, value, "updatedAt" AS updated_at
            "#,
        )
        .bind(asset_id)
        .bind(key)
        .bind(value)
        .fetch_one(pool)
        .await?;
        results.push(row);
    }
    Ok(results)
}

pub async fn delete_by_key(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM asset_metadata WHERE "assetId" = $1 AND key = $2"#)
        .bind(asset_id)
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_bulk(
    pool: &Pool<Postgres>,
    items: &[(Uuid, String)],
) -> Result<(), sqlx::Error> {
    for (asset_id, key) in items {
        delete_by_key(pool, asset_id, key).await?;
    }
    Ok(())
}
