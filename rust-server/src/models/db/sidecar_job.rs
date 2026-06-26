use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct SidecarCheckAsset {
    pub id: Uuid,
    pub original_path: String,
    pub sidecar_path: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct SidecarWriteAsset {
    pub id: Uuid,
    pub original_path: String,
    pub sidecar_path: Option<String>,
    pub description: String,
    pub date_time_original: Option<DateTime<Utc>>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub rating: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub time_zone: Option<String>,
}

pub async fn get_for_sidecar_check(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<SidecarCheckAsset>, sqlx::Error> {
    sqlx::query_as::<_, SidecarCheckAsset>(
        r#"
        SELECT
            asset.id,
            asset."originalPath" AS original_path,
            (
                SELECT af.path
                FROM asset_file af
                WHERE af."assetId" = asset.id
                  AND af.type = 'sidecar'
                  AND af."isEdited" = false
                LIMIT 1
            ) AS sidecar_path
        FROM asset
        WHERE asset.id = $1
        LIMIT 1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn get_for_sidecar_write(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<SidecarWriteAsset>, sqlx::Error> {
    sqlx::query_as::<_, SidecarWriteAsset>(
        r#"
        SELECT
            asset.id,
            asset."originalPath" AS original_path,
            (
                SELECT af.path
                FROM asset_file af
                WHERE af."assetId" = asset.id
                  AND af.type = 'sidecar'
                  AND af."isEdited" = false
                LIMIT 1
            ) AS sidecar_path,
            asset_exif.description,
            asset_exif."dateTimeOriginal" AS date_time_original,
            asset_exif.latitude,
            asset_exif.longitude,
            asset_exif.rating,
            asset_exif.tags,
            asset_exif."timeZone" AS time_zone
        FROM asset
        INNER JOIN asset_exif ON asset.id = asset_exif."assetId"
        WHERE asset.id = $1
        LIMIT 1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn get_locked_properties(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    let value: Option<Vec<String>> = sqlx::query_scalar(
        r#"
        SELECT "lockedProperties"
        FROM asset_exif
        WHERE "assetId" = $1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?;

    Ok(value.unwrap_or_default())
}

pub async fn unlock_properties(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    properties: &[String],
) -> Result<(), sqlx::Error> {
    if properties.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"
        UPDATE asset_exif
        SET "lockedProperties" = nullif(
            array(
                SELECT DISTINCT property
                FROM unnest("lockedProperties") property
                WHERE NOT property = ANY($1)
            ),
            '{}'
        )
        WHERE "assetId" = $2
        "#,
    )
    .bind(properties)
    .bind(asset_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_sidecar_file(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        DELETE FROM asset_file
        WHERE "assetId" = $1 AND type = 'sidecar'
        "#,
    )
    .bind(asset_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn stream_for_sidecar(
    pool: &Pool<Postgres>,
    force: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if force {
        return sqlx::query_scalar(
            r#"
            SELECT asset.id
            FROM asset
            "#,
        )
        .fetch_all(pool)
        .await;
    }

    sqlx::query_scalar(
        r#"
        SELECT asset.id
        FROM asset
        WHERE NOT EXISTS (
            SELECT asset_file.id
            FROM asset_file
            WHERE asset_file."assetId" = asset.id
              AND asset_file.type = 'sidecar'
        )
        "#,
    )
    .fetch_all(pool)
    .await
}
