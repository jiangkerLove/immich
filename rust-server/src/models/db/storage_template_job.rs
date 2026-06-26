use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct StorageTemplateAsset {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub asset_type: String,
    pub checksum: Vec<u8>,
    pub original_path: String,
    pub is_external: bool,
    pub original_file_name: String,
    pub live_photo_video_id: Option<Uuid>,
    pub file_created_at: Option<DateTime<Utc>>,
    pub file_size_in_byte: Option<i64>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens_model: Option<String>,
}

pub async fn get_for_storage_template_job(
    pool: &Pool<Postgres>,
    id: &Uuid,
    include_hidden: bool,
) -> Result<Option<StorageTemplateAsset>, sqlx::Error> {
    let visibility_filter = if include_hidden {
        ""
    } else {
        r#"AND asset.visibility != 'hidden'"#
    };

    let query = format!(
        r#"
        SELECT
            asset.id,
            asset."ownerId" AS owner_id,
            asset.type AS asset_type,
            asset.checksum,
            asset."originalPath" AS original_path,
            asset."isExternal" AS is_external,
            asset."originalFileName" AS original_file_name,
            asset."livePhotoVideoId" AS live_photo_video_id,
            asset."fileCreatedAt" AS file_created_at,
            asset_exif."fileSizeInByte" AS file_size_in_byte,
            asset_exif.make,
            asset_exif.model,
            asset_exif."lensModel" AS lens_model
        FROM asset
        INNER JOIN asset_exif ON asset.id = asset_exif."assetId"
        WHERE asset."deletedAt" IS NULL
          AND asset.id = $1
          {visibility_filter}
        "#
    );

    sqlx::query_as::<_, StorageTemplateAsset>(&query)
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn update_original_path(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE asset
        SET "originalPath" = $2
        WHERE id = $1
        "#,
    )
    .bind(asset_id)
    .bind(path)
    .execute(pool)
    .await?;
    Ok(())
}
