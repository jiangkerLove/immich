use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

use crate::models::db::assets::AssetDetailRow;

#[derive(Debug, FromRow)]
struct DirectoryPathRow {
    directory_path: Option<String>,
}

pub async fn get_unique_original_paths(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_as::<_, DirectoryPathRow>(
        r#"
        SELECT DISTINCT
            substring(asset."originalPath" FROM '^(.*/)[^/]*$') AS directory_path
        FROM asset
        WHERE asset."ownerId" = $1
          AND asset.visibility = 'timeline'
          AND asset."deletedAt" IS NULL
          AND asset."fileCreatedAt" IS NOT NULL
          AND asset."fileModifiedAt" IS NOT NULL
          AND asset."localDateTime" IS NOT NULL
        ORDER BY directory_path ASC
        "#,
    )
    .bind(owner_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| row.directory_path)
        .map(|path| path.trim_end_matches('/').to_string())
        .collect())
}

pub async fn get_assets_by_original_path(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    partial_path: &str,
) -> Result<Vec<AssetDetailRow>, sqlx::Error> {
    let normalized_path = partial_path.trim_end_matches('/');
    let like_pattern = format!("%{normalized_path}/%");
    let exclude_pattern = format!("%{normalized_path}/%/%");

    sqlx::query_as::<_, AssetDetailRow>(
        r#"
            SELECT
                a.id,
                a."ownerId" as owner_id,
                a.type,
                a."originalPath" as original_path,
                a."originalFileName" as original_file_name,
                a."createdAt" as created_at,
                a."updatedAt" as updated_at,
                a."fileCreatedAt" as file_created_at,
                a."fileModifiedAt" as file_modified_at,
                a."localDateTime" as local_date_time,
                a."isFavorite" as is_favorite,
                a.visibility,
                a."deletedAt" as deleted_at,
                a."isOffline" as is_offline,
                a."libraryId" as library_id,
                a."livePhotoVideoId" as live_photo_video_id,
                a."duplicateId" as duplicate_id,
                a.duration,
                a.thumbhash,
                a.checksum,
                a.width,
                a.height,
                a."isEdited" as is_edited,
                a."stackId" as stack_id,
                u.name as owner_name,
                u.email as owner_email,
                u."profileImagePath" as owner_profile_image_path,
                u."avatarColor" as owner_avatar_color,
                u."profileChangedAt" as owner_profile_changed_at,
                (
                    SELECT to_json(e.*)
                    FROM asset_exif e
                    WHERE e."assetId" = a.id
                ) as exif_json,
                (
                    SELECT COALESCE(json_agg(json_build_object(
                        'id', t.id,
                        'value', t.value,
                        'color', t.color,
                        'createdAt', t."createdAt",
                        'updatedAt', t."updatedAt",
                        'parentId', t."parentId"
                    )), '[]'::json)
                    FROM tag t
                    INNER JOIN tag_asset ta ON ta."tagId" = t.id
                    WHERE ta."assetId" = a.id
                ) as tags_json
            FROM asset a
            INNER JOIN "user" u ON u.id = a."ownerId"
            WHERE a."ownerId" = $1
              AND a.visibility = 'timeline'
              AND a."deletedAt" IS NULL
              AND a."fileCreatedAt" IS NOT NULL
              AND a."fileModifiedAt" IS NOT NULL
              AND a."localDateTime" IS NOT NULL
              AND a."originalPath" LIKE $2
              AND a."originalPath" NOT LIKE $3
            ORDER BY regexp_replace(a."originalPath", '.*/(.+)', '\1') ASC
        "#,
    )
    .bind(owner_id)
    .bind(like_pattern)
    .bind(exclude_pattern)
    .fetch_all(pool)
    .await
}
