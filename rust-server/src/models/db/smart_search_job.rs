use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ClipEncodingAsset {
    pub id: Uuid,
    pub visibility: String,
    pub preview_path: Option<String>,
}

pub async fn get_for_clip_encoding(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<ClipEncodingAsset>, sqlx::Error> {
    sqlx::query_as::<_, ClipEncodingAsset>(
        r#"
        SELECT
            asset.id,
            asset.visibility,
            (
                SELECT af.path
                FROM asset_file af
                WHERE af."assetId" = asset.id
                  AND af.type = 'preview'
                  AND af."isEdited" = false
                LIMIT 1
            ) AS preview_path
        FROM asset
        WHERE asset.id = $1
        LIMIT 1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn stream_for_encode_clip(
    pool: &Pool<Postgres>,
    force: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if force {
        return sqlx::query_scalar(
            r#"
            SELECT asset.id
            FROM asset
            INNER JOIN asset_job_status ON asset_job_status."assetId" = asset.id
            WHERE asset."deletedAt" IS NULL
              AND asset.visibility != 'hidden'
              AND EXISTS (
                SELECT 1
                FROM asset_file
                WHERE "assetId" = asset.id
                  AND type = 'preview'
              )
            "#,
        )
        .fetch_all(pool)
        .await;
    }

    sqlx::query_scalar(
        r#"
        SELECT asset.id
        FROM asset
        INNER JOIN asset_job_status ON asset_job_status."assetId" = asset.id
        WHERE asset."deletedAt" IS NULL
          AND asset.visibility != 'hidden'
          AND EXISTS (
            SELECT 1
            FROM asset_file
            WHERE "assetId" = asset.id
              AND type = 'preview'
          )
          AND NOT EXISTS (
            SELECT 1
            FROM smart_search
            WHERE "assetId" = asset.id
          )
        "#,
    )
    .fetch_all(pool)
    .await
}
