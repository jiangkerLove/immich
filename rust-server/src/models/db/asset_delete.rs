use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct AssetDeletionRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub library_id: Option<Uuid>,
    pub live_photo_video_id: Option<Uuid>,
    pub original_path: String,
    pub is_offline: bool,
    pub stack_id: Option<Uuid>,
    pub primary_asset_id: Option<Uuid>,
    pub file_size: Option<i64>,
}

#[derive(Debug, FromRow)]
struct AssetFilePathRow {
    path: String,
}

pub async fn list_trashed_before(
    pool: &Pool<Postgres>,
    before: DateTime<Utc>,
) -> Result<Vec<(Uuid, bool)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, bool)>(
        r#"
            SELECT id, "isOffline"
            FROM asset
            WHERE "deletedAt" <= $1
        "#,
    )
    .bind(before)
    .fetch_all(pool)
    .await
}

pub async fn list_status_deleted(pool: &Pool<Postgres>) -> Result<Vec<Uuid>, sqlx::Error> {
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

pub async fn get_for_deletion(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<AssetDeletionRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetDeletionRow>(
        r#"
            SELECT
                asset.id,
                asset."ownerId" as owner_id,
                asset."libraryId" as library_id,
                asset."livePhotoVideoId" as live_photo_video_id,
                asset."originalPath" as original_path,
                asset."isOffline" as is_offline,
                asset."stackId" as stack_id,
                stack."primaryAssetId" as primary_asset_id,
                asset_exif."fileSizeInByte" as file_size
            FROM asset
            LEFT JOIN stack ON stack.id = asset."stackId"
            LEFT JOIN asset_exif ON asset_exif."assetId" = asset.id
            WHERE asset.id = $1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn list_stack_timeline_asset_ids(
    pool: &Pool<Postgres>,
    stack_id: &Uuid,
    exclude_asset_id: &Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT id
            FROM asset
            WHERE "stackId" = $1
              AND id != $2
              AND visibility = 'timeline'
              AND status != 'deleted'
        "#,
    )
    .bind(stack_id)
    .bind(exclude_asset_id)
    .fetch_all(pool)
    .await
}

pub async fn list_asset_file_paths(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_as::<_, AssetFilePathRow>(
        r#"SELECT path FROM asset_file WHERE "assetId" = $1"#,
    )
    .bind(asset_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(|row| row.path).collect())
}

pub async fn count_live_photo_references(
    pool: &Pool<Postgres>,
    live_photo_video_id: &Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT COUNT(*)
            FROM asset
            WHERE "livePhotoVideoId" = $1
        "#,
    )
    .bind(live_photo_video_id)
    .fetch_one(pool)
    .await
}

pub async fn hard_delete(pool: &Pool<Postgres>, asset_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM asset WHERE id = $1"#)
        .bind(asset_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_deleted_by_library(
    pool: &Pool<Postgres>,
    library_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE asset
            SET "deletedAt" = NOW(), "updatedAt" = NOW()
            WHERE "libraryId" = $1
        "#,
    )
    .bind(library_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_ids_by_library(
    pool: &Pool<Postgres>,
    library_id: &Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT id
            FROM asset
            WHERE "libraryId" = $1
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
}

pub async fn library_has_assets(
    pool: &Pool<Postgres>,
    library_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM asset WHERE "libraryId" = $1"#,
    )
    .bind(library_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}
