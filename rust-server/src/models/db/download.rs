use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct DownloadAssetRow {
    pub id: Uuid,
    pub live_photo_video_id: Option<Uuid>,
    pub size: Option<i64>,
}

#[derive(Debug, FromRow)]
pub struct DownloadMotionRow {
    pub id: Uuid,
    pub original_path: String,
    pub size: Option<i64>,
}

pub async fn download_asset_ids(
    pool: &Pool<Postgres>,
    ids: &[Uuid],
) -> Result<Vec<DownloadAssetRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_as::<_, DownloadAssetRow>(
        r#"
        SELECT
            asset.id,
            asset."livePhotoVideoId" AS live_photo_video_id,
            asset_exif."fileSizeInByte" AS size
        FROM asset
        INNER JOIN asset_exif ON asset_exif."assetId" = asset.id
        WHERE asset."deletedAt" IS NULL
          AND asset.id = ANY($1)
        "#,
    )
    .bind(ids)
    .fetch_all(pool)
    .await
}

pub async fn download_album_id(
    pool: &Pool<Postgres>,
    album_id: &Uuid,
) -> Result<Vec<DownloadAssetRow>, sqlx::Error> {
    sqlx::query_as::<_, DownloadAssetRow>(
        r#"
        SELECT
            asset.id,
            asset."livePhotoVideoId" AS live_photo_video_id,
            asset_exif."fileSizeInByte" AS size
        FROM asset
        INNER JOIN asset_exif ON asset_exif."assetId" = asset.id
        INNER JOIN album_asset ON album_asset."assetId" = asset.id
        WHERE asset."deletedAt" IS NULL
          AND album_asset."albumId" = $1
        "#,
    )
    .bind(album_id)
    .fetch_all(pool)
    .await
}

pub async fn download_user_id(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
) -> Result<Vec<DownloadAssetRow>, sqlx::Error> {
    sqlx::query_as::<_, DownloadAssetRow>(
        r#"
        SELECT
            asset.id,
            asset."livePhotoVideoId" AS live_photo_video_id,
            asset_exif."fileSizeInByte" AS size
        FROM asset
        INNER JOIN asset_exif ON asset_exif."assetId" = asset.id
        WHERE asset."deletedAt" IS NULL
          AND asset."ownerId" = $1
          AND asset.visibility != 'hidden'
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn download_motion_asset_ids(
    pool: &Pool<Postgres>,
    ids: &[Uuid],
) -> Result<Vec<DownloadMotionRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_as::<_, DownloadMotionRow>(
        r#"
        SELECT
            asset.id,
            asset."originalPath" AS original_path,
            asset_exif."fileSizeInByte" AS size
        FROM asset
        INNER JOIN asset_exif ON asset_exif."assetId" = asset.id
        WHERE asset."deletedAt" IS NULL
          AND asset.id = ANY($1)
        "#,
    )
    .bind(ids)
    .fetch_all(pool)
    .await
}
