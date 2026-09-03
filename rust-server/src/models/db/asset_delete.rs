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

#[derive(Debug, FromRow, Clone, PartialEq, Eq)]
pub struct AssetFilePathRow {
    pub file_type: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackDeleteAction {
    Keep,
    Delete,
    PromoteFirst,
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

pub async fn list_asset_files_for_deletion(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Vec<AssetFilePathRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetFilePathRow>(
        r#"SELECT type AS file_type, path FROM asset_file WHERE "assetId" = $1"#,
    )
    .bind(asset_id)
    .fetch_all(pool)
    .await
}

pub fn deletion_file_paths(
    files: &[AssetFilePathRow],
    original_path: &str,
    delete_on_disk: bool,
    is_offline: bool,
) -> Vec<String> {
    let include_original_and_sidecar = delete_on_disk && !is_offline;
    let mut paths: Vec<String> = files
        .iter()
        .filter(|file| !file.path.is_empty())
        .filter(|file| file.file_type != "sidecar" || include_original_and_sidecar)
        .map(|file| file.path.clone())
        .collect();
    if include_original_and_sidecar && !original_path.is_empty() {
        paths.push(original_path.to_string());
    }
    paths
}

pub fn stack_action_after_asset_delete(
    deleted_is_primary: bool,
    remaining_timeline_count: usize,
) -> StackDeleteAction {
    if remaining_timeline_count < 2 {
        StackDeleteAction::Delete
    } else if deleted_is_primary {
        StackDeleteAction::PromoteFirst
    } else {
        StackDeleteAction::Keep
    }
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
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM asset WHERE "libraryId" = $1"#)
        .bind(library_id)
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(file_type: &str, path: &str) -> AssetFilePathRow {
        AssetFilePathRow {
            file_type: file_type.to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn deletion_keeps_derivatives_and_skips_sidecar_unless_deleting_original() {
        let files = [
            file("thumbnail", "/thumbs/a.webp"),
            file("sidecar", "/library/a.jpg.xmp"),
            file("encoded_video", "/encoded/a.mp4"),
        ];

        let keep_original = deletion_file_paths(&files, "/library/a.jpg", false, false);
        assert_eq!(
            keep_original,
            vec!["/thumbs/a.webp".to_string(), "/encoded/a.mp4".to_string()]
        );

        let delete_original = deletion_file_paths(&files, "/library/a.jpg", true, false);
        assert_eq!(
            delete_original,
            vec![
                "/thumbs/a.webp".to_string(),
                "/library/a.jpg.xmp".to_string(),
                "/encoded/a.mp4".to_string(),
                "/library/a.jpg".to_string(),
            ]
        );

        let offline = deletion_file_paths(&files, "/library/a.jpg", true, true);
        assert_eq!(
            offline,
            vec!["/thumbs/a.webp".to_string(), "/encoded/a.mp4".to_string()]
        );
    }

    #[test]
    fn stack_dissolves_when_fewer_than_two_timeline_assets_remain() {
        assert_eq!(
            stack_action_after_asset_delete(true, 1),
            StackDeleteAction::Delete
        );
        assert_eq!(
            stack_action_after_asset_delete(false, 1),
            StackDeleteAction::Delete
        );
        assert_eq!(
            stack_action_after_asset_delete(true, 2),
            StackDeleteAction::PromoteFirst
        );
        assert_eq!(
            stack_action_after_asset_delete(false, 2),
            StackDeleteAction::Keep
        );
    }
}
