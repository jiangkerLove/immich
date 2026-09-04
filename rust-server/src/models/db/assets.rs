use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct AssetOriginalRow {
    pub id: Uuid,
    pub original_file_name: String,
    pub original_path: String,
    pub edited_path: Option<String>,
}

#[derive(Debug, FromRow)]
pub struct AssetThumbnailRow {
    pub original_path: String,
    pub original_file_name: String,
    pub path: Option<String>,
}

#[derive(Debug, FromRow)]
pub struct AssetVideoRow {
    pub original_path: String,
    pub encoded_video_path: Option<String>,
}

#[derive(Debug, FromRow)]
pub struct AssetChecksumRow {
    pub id: Uuid,
    pub checksum: Vec<u8>,
    pub deleted_at: Option<DateTime<Utc>>,
}

pub async fn owner_has_asset(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    asset_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM asset WHERE id = $1 AND "ownerId" = $2 AND "deletedAt" IS NULL)"#,
    )
    .bind(asset_id)
    .bind(owner_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

pub async fn shared_link_has_asset(
    pool: &Pool<Postgres>,
    shared_link_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<bool, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(true);
    }
    let count: i64 = sqlx::query_scalar(
        r#"
            SELECT COUNT(DISTINCT matched_id)
            FROM (
                SELECT unnest(array[
                    asset.id,
                    asset."livePhotoVideoId",
                    "albumAssets".id,
                    "albumAssets"."livePhotoVideoId"
                ]) AS matched_id
                FROM shared_link
                LEFT JOIN album ON album.id = shared_link."albumId" AND album."deletedAt" IS NULL
                LEFT JOIN shared_link_asset ON shared_link_asset."sharedLinkId" = shared_link.id
                LEFT JOIN asset ON asset.id = shared_link_asset."assetId" AND asset."deletedAt" IS NULL
                LEFT JOIN album_asset ON album_asset."albumId" = album.id
                LEFT JOIN asset AS "albumAssets" ON "albumAssets".id = album_asset."assetId" AND "albumAssets"."deletedAt" IS NULL
                WHERE shared_link.id = $1
            ) sub
            WHERE matched_id = ANY($2)
        "#,
    )
    .bind(shared_link_id)
    .bind(asset_ids)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

pub async fn get_for_original(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    edited: bool,
) -> Result<Option<AssetOriginalRow>, sqlx::Error> {
    get_for_originals(pool, &[*asset_id], edited)
        .await
        .map(|rows| rows.into_iter().next())
}

pub async fn get_for_originals(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
    edited: bool,
) -> Result<Vec<AssetOriginalRow>, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_as::<_, AssetOriginalRow>(
        r#"
            SELECT
                asset.id,
                asset."originalFileName" as original_file_name,
                asset."originalPath" as original_path,
                af.path as edited_path
            FROM asset
            LEFT JOIN asset_file af ON asset.id = af."assetId"
                AND af."isEdited" = $1
                AND af.type = 'fullsize'
            WHERE asset.id = ANY($2) AND asset."deletedAt" IS NULL
        "#,
    )
    .bind(edited)
    .bind(asset_ids)
    .fetch_all(pool)
    .await
}

pub async fn get_for_thumbnail(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    file_type: &str,
) -> Result<Option<AssetThumbnailRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetThumbnailRow>(
        r#"
            SELECT asset."originalPath" as original_path,
                   asset."originalFileName" as original_file_name,
                   af.path
            FROM asset
            LEFT JOIN asset_file af ON asset.id = af."assetId" AND af.type = $1
            WHERE asset.id = $2 AND asset."deletedAt" IS NULL
            ORDER BY af."isEdited" DESC
            LIMIT 1
        "#,
    )
    .bind(file_type)
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_for_video(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<AssetVideoRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetVideoRow>(
        r#"
            SELECT
                asset."originalPath" as original_path,
                (
                    SELECT af.path FROM asset_file af
                    WHERE af."assetId" = asset.id
                      AND af.type = 'encoded_video'
                      AND af."isEdited" = false
                    LIMIT 1
                ) as encoded_video_path
            FROM asset
            WHERE asset.id = $1 AND asset."deletedAt" IS NULL AND asset.type = 'VIDEO'
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_upload_id_by_checksum(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    checksum: &[u8],
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT id FROM asset
            WHERE "ownerId" = $1 AND checksum = $2 AND "libraryId" IS NULL
            LIMIT 1
        "#,
    )
    .bind(owner_id)
    .bind(checksum)
    .fetch_optional(pool)
    .await
}

pub async fn get_by_checksum(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    library_id: Option<&Uuid>,
    checksum: &[u8],
) -> Result<Option<Uuid>, sqlx::Error> {
    if let Some(library_id) = library_id {
        sqlx::query_scalar(
            r#"
            SELECT id FROM asset
            WHERE "ownerId" = $1 AND "libraryId" = $2 AND checksum = $3
            LIMIT 1
            "#,
        )
        .bind(owner_id)
        .bind(library_id)
        .bind(checksum)
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query_scalar(
            r#"
            SELECT id FROM asset
            WHERE "ownerId" = $1 AND "libraryId" IS NULL AND checksum = $2
            LIMIT 1
            "#,
        )
        .bind(owner_id)
        .bind(checksum)
        .fetch_optional(pool)
        .await
    }
}

pub async fn get_by_checksums(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    checksums: &[Vec<u8>],
) -> Result<Vec<AssetChecksumRow>, sqlx::Error> {
    if checksums.is_empty() {
        return Ok(vec![]);
    }
    sqlx::query_as::<_, AssetChecksumRow>(
        r#"
            SELECT id, checksum, "deletedAt" as deleted_at
            FROM asset
            WHERE "ownerId" = $1 AND checksum = ANY($2)
        "#,
    )
    .bind(owner_id)
    .bind(checksums)
    .fetch_all(pool)
    .await
}

#[derive(Debug)]
pub struct NewAsset<'a> {
    pub owner_id: Uuid,
    pub asset_type: &'a str,
    pub original_path: &'a str,
    pub checksum: &'a [u8],
    pub file_created_at: DateTime<Utc>,
    pub file_modified_at: DateTime<Utc>,
    pub is_favorite: bool,
    pub duration: Option<i32>,
    pub original_file_name: &'a str,
    pub live_photo_video_id: Option<Uuid>,
    pub visibility: &'a str,
}

pub async fn create_asset(pool: &Pool<Postgres>, asset: NewAsset<'_>) -> Result<Uuid, sqlx::Error> {
    let id: Uuid = sqlx::query_scalar(
        r#"
            INSERT INTO asset (
                "ownerId", type, "originalPath", checksum, "checksumAlgorithm",
                "fileCreatedAt", "fileModifiedAt", "localDateTime",
                "isFavorite", duration, "originalFileName", "livePhotoVideoId", visibility
            )
            VALUES ($1, $2, $3, $4, 'sha1', $5, $6, $5, $7, $8, $9, $10, $11::asset_visibility_enum)
            RETURNING id
        "#,
    )
    .bind(asset.owner_id)
    .bind(asset.asset_type)
    .bind(asset.original_path)
    .bind(asset.checksum)
    .bind(asset.file_created_at)
    .bind(asset.file_modified_at)
    .bind(asset.is_favorite)
    .bind(asset.duration)
    .bind(asset.original_file_name)
    .bind(asset.live_photo_video_id)
    .bind(asset.visibility)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

#[derive(Debug)]
pub struct NewLibraryAsset<'a> {
    pub owner_id: Uuid,
    pub library_id: Uuid,
    pub asset_type: &'a str,
    pub original_path: &'a str,
    pub checksum: &'a [u8],
    pub file_created_at: DateTime<Utc>,
    pub file_modified_at: DateTime<Utc>,
    pub original_file_name: &'a str,
}

pub async fn create_library_asset(
    pool: &Pool<Postgres>,
    asset: NewLibraryAsset<'_>,
) -> Result<Uuid, sqlx::Error> {
    let id: Uuid = sqlx::query_scalar(
        r#"
            INSERT INTO asset (
                "ownerId", "libraryId", type, "originalPath", checksum, "checksumAlgorithm",
                "fileCreatedAt", "fileModifiedAt", "localDateTime",
                "originalFileName", "isExternal", visibility
            )
            VALUES ($1, $2, $3, $4, $5, 'sha1-path', $6, $7, $6, $8, true, 'timeline'::asset_visibility_enum)
            RETURNING id
        "#,
    )
    .bind(asset.owner_id)
    .bind(asset.library_id)
    .bind(asset.asset_type)
    .bind(asset.original_path)
    .bind(asset.checksum)
    .bind(asset.file_created_at)
    .bind(asset.file_modified_at)
    .bind(asset.original_file_name)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

pub async fn filter_new_external_paths(
    pool: &Pool<Postgres>,
    library_id: &Uuid,
    paths: &[String],
) -> Result<Vec<String>, sqlx::Error> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    sqlx::query_scalar(
        r#"
            SELECT path
            FROM UNNEST($2::text[]) AS path
            WHERE NOT EXISTS (
                SELECT 1
                FROM asset
                WHERE asset."originalPath" = path
                  AND asset."libraryId" = $1
                  AND asset."isExternal" = true
            )
        "#,
    )
    .bind(library_id)
    .bind(paths)
    .fetch_all(pool)
    .await
}

pub async fn upsert_exif_size(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    size: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            INSERT INTO asset_exif ("assetId", "fileSizeInByte")
            VALUES ($1, $2)
            ON CONFLICT ("assetId") DO UPDATE SET "fileSizeInByte" = EXCLUDED."fileSizeInByte"
        "#,
    )
    .bind(asset_id)
    .bind(size)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_quota_usage(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    delta: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE "user" SET "quotaUsageInBytes" = "quotaUsageInBytes" + $1 WHERE id = $2"#,
    )
    .bind(delta)
    .bind(owner_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
pub struct AssetDetailRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    #[sqlx(rename = "type")]
    pub asset_type: String,
    pub original_path: String,
    pub original_file_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub file_created_at: DateTime<Utc>,
    pub file_modified_at: DateTime<Utc>,
    pub local_date_time: DateTime<Utc>,
    pub is_favorite: bool,
    pub visibility: String,
    pub deleted_at: Option<DateTime<Utc>>,
    pub is_offline: bool,
    pub library_id: Option<Uuid>,
    pub live_photo_video_id: Option<Uuid>,
    pub duplicate_id: Option<Uuid>,
    pub duration: Option<i32>,
    pub thumbhash: Option<Vec<u8>>,
    pub checksum: Vec<u8>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub is_edited: bool,
    pub stack_id: Option<Uuid>,
    pub owner_name: String,
    pub owner_email: String,
    pub owner_profile_image_path: String,
    pub owner_avatar_color: Option<String>,
    pub owner_profile_changed_at: DateTime<Utc>,
    pub exif_json: Option<serde_json::Value>,
    pub tags_json: Option<serde_json::Value>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct AssetStackRow {
    pub id: Uuid,
    pub primary_asset_id: Uuid,
    pub asset_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct AssetStatsRow {
    #[sqlx(rename = "IMAGE")]
    pub image: i64,
    #[sqlx(rename = "VIDEO")]
    pub video: i64,
    #[sqlx(rename = "AUDIO")]
    pub audio: i64,
    #[sqlx(rename = "OTHER")]
    pub other: i64,
}

pub async fn get_detail_by_id(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<AssetDetailRow>, sqlx::Error> {
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
            WHERE a.id = $1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_details_by_ids(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
) -> Result<Vec<AssetDetailRow>, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(vec![]);
    }
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
            WHERE a.id = ANY($1) AND a."deletedAt" IS NULL
            ORDER BY a."fileCreatedAt" ASC
        "#,
    )
    .bind(asset_ids)
    .fetch_all(pool)
    .await
}

pub async fn filter_asset_share_ids(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_scalar(
        r#"
            SELECT DISTINCT id FROM (
                SELECT asset.id
                FROM asset
                WHERE asset.id = ANY($1)
                  AND asset."ownerId" = $2
                UNION
                SELECT asset.id
                FROM partner
                INNER JOIN "user" AS shared_by ON shared_by.id = partner."sharedById" AND shared_by."deletedAt" IS NULL
                INNER JOIN asset ON asset."ownerId" = shared_by.id AND asset."deletedAt" IS NULL
                WHERE partner."sharedWithId" = $2
                  AND asset.id = ANY($1)
                  AND asset.visibility IN ('timeline', 'hidden')
            ) accessible
        "#,
    )
    .bind(asset_ids)
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn get_stack(
    pool: &Pool<Postgres>,
    stack_id: &Uuid,
) -> Result<Option<AssetStackRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetStackRow>(
        r#"
            SELECT
                s.id,
                s."primaryAssetId" as primary_asset_id,
                (
                    SELECT COUNT(*)
                    FROM asset stacked
                    WHERE stacked."stackId" = s.id
                      AND stacked."deletedAt" IS NULL
                      AND stacked.visibility = 'timeline'
                ) + 1 AS asset_count
            FROM stack s
            WHERE s.id = $1
        "#,
    )
    .bind(stack_id)
    .fetch_optional(pool)
    .await
}

pub async fn filter_accessible_ids(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    asset_ids: &[Uuid],
    elevated: bool,
    owner_only: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(vec![]);
    }

    if owner_only {
        return sqlx::query_scalar(
            r#"
                SELECT id FROM asset
                WHERE id = ANY($1)
                  AND "ownerId" = $2
                  AND ($3 OR visibility != 'locked')
            "#,
        )
        .bind(asset_ids)
        .bind(user_id)
        .bind(elevated)
        .fetch_all(pool)
        .await;
    }

    sqlx::query_scalar(
        r#"
            SELECT DISTINCT id FROM (
                SELECT asset.id
                FROM asset
                WHERE asset.id = ANY($1)
                  AND asset."ownerId" = $2
                  AND ($3 OR asset.visibility != 'locked')
                UNION
                SELECT asset.id
                FROM album
                INNER JOIN album_asset ON album.id = album_asset."albumId"
                INNER JOIN asset ON asset.id = album_asset."assetId" AND asset."deletedAt" IS NULL
                INNER JOIN album_user ON album_user."albumId" = album.id
                INNER JOIN "user" ON "user".id = album_user."userId" AND "user"."deletedAt" IS NULL
                WHERE (
                    asset.id = ANY($1)
                    OR asset."livePhotoVideoId" = ANY($1)
                )
                  AND album_user."userId" = $2
                  AND album."deletedAt" IS NULL
                UNION
                SELECT asset.id
                FROM partner
                INNER JOIN "user" AS shared_by ON shared_by.id = partner."sharedById" AND shared_by."deletedAt" IS NULL
                INNER JOIN asset ON asset."ownerId" = shared_by.id AND asset."deletedAt" IS NULL
                WHERE partner."sharedWithId" = $2
                  AND asset.id = ANY($1)
                  AND asset.visibility IN ('timeline', 'hidden')
            ) accessible
        "#,
    )
    .bind(asset_ids)
    .bind(user_id)
    .bind(elevated)
    .fetch_all(pool)
    .await
}

pub async fn shared_link_accessible_ids(
    pool: &Pool<Postgres>,
    shared_link_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_scalar(
        r#"
            SELECT DISTINCT matched_id
            FROM (
                SELECT unnest(array[
                    asset.id,
                    asset."livePhotoVideoId",
                    album_assets.id,
                    album_assets."livePhotoVideoId"
                ]) AS matched_id
                FROM shared_link
                LEFT JOIN album ON album.id = shared_link."albumId" AND album."deletedAt" IS NULL
                LEFT JOIN shared_link_asset ON shared_link_asset."sharedLinkId" = shared_link.id
                LEFT JOIN asset ON asset.id = shared_link_asset."assetId" AND asset."deletedAt" IS NULL
                LEFT JOIN album_asset ON album_asset."albumId" = album.id
                LEFT JOIN asset AS album_assets ON album_assets.id = album_asset."assetId" AND album_assets."deletedAt" IS NULL
                WHERE shared_link.id = $1
            ) sub
            WHERE matched_id = ANY($2)
        "#,
    )
    .bind(shared_link_id)
    .bind(asset_ids)
    .fetch_all(pool)
    .await
}

pub async fn get_statistics(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    visibility: Option<&str>,
    is_favorite: Option<bool>,
    is_trashed: bool,
) -> Result<AssetStatsRow, sqlx::Error> {
    let mut query = sqlx::QueryBuilder::new(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE type = 'IMAGE') AS "IMAGE",
            COUNT(*) FILTER (WHERE type = 'VIDEO') AS "VIDEO",
            COUNT(*) FILTER (WHERE type = 'AUDIO') AS "AUDIO",
            COUNT(*) FILTER (WHERE type = 'OTHER') AS "OTHER"
        FROM asset
        WHERE "ownerId" =
        "#,
    );
    query.push_bind(owner_id);

    if is_trashed {
        query.push(r#" AND "deletedAt" IS NOT NULL AND status != 'deleted'"#);
    } else {
        query.push(r#" AND "deletedAt" IS NULL"#);
    }

    if let Some(visibility) = visibility {
        crate::utils::query::push_visibility_enum_eq(&mut query, "AND visibility", visibility);
    } else {
        query.push(r#" AND visibility IN ('archive', 'timeline')"#);
    }

    if let Some(is_favorite) = is_favorite {
        query.push(r#" AND "isFavorite" = "#);
        query.push_bind(is_favorite);
    }

    query
        .build_query_as::<AssetStatsRow>()
        .fetch_one(pool)
        .await
}

#[derive(Debug, sqlx::FromRow)]
pub struct CalendarHeatmapRow {
    pub date: DateTime<Utc>,
    pub count: i64,
}

pub async fn get_calendar_heatmap(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    taken_at: bool,
) -> Result<Vec<CalendarHeatmapRow>, sqlx::Error> {
    let date_column = if taken_at {
        r#""localDateTime""#
    } else {
        r#""createdAt""#
    };

    let sql = format!(
        r#"
        SELECT
            date_trunc('day', asset.{date_column} AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' AS date,
            COUNT(*)::bigint AS count
        FROM asset
        WHERE asset."ownerId" = $1
          AND asset.{date_column} >= $2
          AND asset.{date_column} < $3
          AND asset."deletedAt" IS NULL
        GROUP BY 1
        ORDER BY 1 ASC
        "#
    );

    sqlx::query_as::<_, CalendarHeatmapRow>(&sql)
        .bind(owner_id)
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await
}

#[derive(Debug, Default)]
pub struct AssetUpdateFields {
    pub is_favorite: Option<bool>,
    pub visibility: Option<String>,
    pub live_photo_video_id: Option<Option<Uuid>>,
    pub duplicate_id: Option<Option<Uuid>>,
}

pub async fn update_asset_fields(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    fields: &AssetUpdateFields,
) -> Result<(), sqlx::Error> {
    let mut query = sqlx::QueryBuilder::new(r#"UPDATE asset SET "#);
    let mut separated = query.separated(", ");

    if let Some(is_favorite) = fields.is_favorite {
        separated.push(r#""isFavorite" = "#);
        separated.push_bind(is_favorite);
    }
    if let Some(visibility) = &fields.visibility {
        separated.push("visibility = ");
        separated.push_bind(visibility.clone());
        separated.push("::asset_visibility_enum");
    }
    if let Some(live_photo_video_id) = &fields.live_photo_video_id {
        separated.push(r#""livePhotoVideoId" = "#);
        separated.push_bind(*live_photo_video_id);
    }
    if let Some(duplicate_id) = &fields.duplicate_id {
        separated.push(r#""duplicateId" = "#);
        separated.push_bind(*duplicate_id);
    }

    query.push(r#" WHERE id = "#);
    query.push_bind(*asset_id);
    query.build().execute(pool).await?;
    Ok(())
}

pub async fn update_all_asset_fields(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
    is_favorite: Option<bool>,
    visibility: Option<&str>,
    duplicate_id: Option<Option<Uuid>>,
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    let mut query = sqlx::QueryBuilder::new(r#"UPDATE asset SET "#);
    let mut separated = query.separated(", ");

    if let Some(is_favorite) = is_favorite {
        separated.push(r#""isFavorite" = "#);
        separated.push_bind(is_favorite);
    }
    if let Some(visibility) = visibility {
        separated.push("visibility = ");
        separated.push_bind(visibility);
        separated.push("::asset_visibility_enum");
    }
    if let Some(duplicate_id) = duplicate_id {
        separated.push(r#""duplicateId" = "#);
        separated.push_bind(duplicate_id);
    }

    query.push(r#" WHERE id = ANY("#);
    query.push_bind(asset_ids);
    query.push("::uuid[])");
    query.build().execute(pool).await?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct ExifUpdateFields {
    pub description: Option<String>,
    pub date_time_original: Option<DateTime<Utc>>,
    pub time_zone: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub rating: Option<Option<i32>>,
}

impl ExifUpdateFields {
    pub fn has_updates(&self) -> bool {
        self.description.is_some()
            || self.date_time_original.is_some()
            || self.time_zone.is_some()
            || self.latitude.is_some()
            || self.longitude.is_some()
            || self.rating.is_some()
    }

    /// Lockable property names matching TS `LockableProperty` / `updateLockedColumns`.
    pub fn locked_property_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.description.is_some() {
            names.push("description");
        }
        if self.date_time_original.is_some() {
            names.push("dateTimeOriginal");
        }
        if self.time_zone.is_some() {
            names.push("timeZone");
        }
        if self.latitude.is_some() {
            names.push("latitude");
        }
        if self.longitude.is_some() {
            names.push("longitude");
        }
        if self.rating.is_some() {
            names.push("rating");
        }
        names
    }
}

/// Append distinct lockable property names (TS `distinctLocked` / append behavior).
pub async fn append_locked_properties(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
    properties: &[&str],
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() || properties.is_empty() {
        return Ok(());
    }

    let props: Vec<String> = properties
        .iter()
        .map(|value| (*value).to_string())
        .collect();
    sqlx::query(
        r#"
            UPDATE asset_exif
            SET "lockedProperties" = nullif(
                array(
                    SELECT DISTINCT unnest(
                        coalesce("lockedProperties", '{}'::text[]) || $1::text[]
                    )
                ),
                '{}'
            )
            WHERE "assetId" = ANY($2::uuid[])
        "#,
    )
    .bind(&props)
    .bind(asset_ids)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, FromRow)]
pub struct AssetBasicRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub asset_type: String,
    pub visibility: String,
    pub original_path: String,
    pub live_photo_video_id: Option<Uuid>,
}

pub fn is_android_motion_path(original_path: &str) -> bool {
    original_path.contains("/encoded-video/")
}

pub async fn get_basic_by_id(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<AssetBasicRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetBasicRow>(
        r#"
            SELECT
                id,
                "ownerId" as owner_id,
                type as asset_type,
                visibility,
                "originalPath" as original_path,
                "livePhotoVideoId" as live_photo_video_id
            FROM asset
            WHERE id = $1 AND "deletedAt" IS NULL
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn update_date_time_relative(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
    delta_minutes: Option<i32>,
    time_zone: Option<&str>,
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    let delta = delta_minutes.unwrap_or(0);
    if delta == 0 && time_zone.is_none() {
        return Ok(());
    }

    let mut query = sqlx::QueryBuilder::new(
        r#"
            UPDATE asset_exif SET
                "dateTimeOriginal" = "dateTimeOriginal" + (
        "#,
    );
    query.push_bind(format!("{delta} minute"));
    query.push(
        r#"
                )::interval
        "#,
    );

    if let Some(time_zone) = time_zone {
        query.push(r#", "timeZone" = "#);
        query.push_bind(time_zone);
    }

    query.push(r#" WHERE "assetId" = ANY("#);
    query.push_bind(asset_ids);
    query.push("::uuid[])");
    query.build().execute(pool).await?;

    // Match TS updateDateTimeOriginal: always lock both columns when relative update runs.
    append_locked_properties(pool, asset_ids, &["dateTimeOriginal", "timeZone"]).await?;
    Ok(())
}

pub async fn remove_assets_from_all_albums(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    sqlx::query(r#"DELETE FROM album_asset WHERE "assetId" = ANY($1)"#)
        .bind(asset_ids)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_exif_fields(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    fields: &ExifUpdateFields,
) -> Result<(), sqlx::Error> {
    let mut query = sqlx::QueryBuilder::new(r#"UPDATE asset_exif SET "#);
    let mut separated = query.separated(", ");

    if let Some(description) = &fields.description {
        separated.push("description = ");
        separated.push_bind(description.clone());
    }
    if let Some(date_time_original) = fields.date_time_original {
        separated.push(r#""dateTimeOriginal" = "#);
        separated.push_bind(date_time_original);
    }
    if let Some(time_zone) = &fields.time_zone {
        separated.push(r#""timeZone" = "#);
        separated.push_bind(time_zone.clone());
    }
    if let Some(latitude) = fields.latitude {
        separated.push("latitude = ");
        separated.push_bind(latitude);
    }
    if let Some(longitude) = fields.longitude {
        separated.push("longitude = ");
        separated.push_bind(longitude);
    }
    if let Some(rating) = &fields.rating {
        separated.push("rating = ");
        separated.push_bind(*rating);
    }

    query.push(r#" WHERE "assetId" = "#);
    query.push_bind(*asset_id);
    query.build().execute(pool).await?;

    let locked = fields.locked_property_names();
    if !locked.is_empty() {
        append_locked_properties(pool, &[*asset_id], &locked).await?;
    }
    Ok(())
}

pub async fn update_all_exif_fields(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
    fields: &ExifUpdateFields,
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    let mut query = sqlx::QueryBuilder::new(r#"UPDATE asset_exif SET "#);
    let mut separated = query.separated(", ");

    if let Some(description) = &fields.description {
        separated.push("description = ");
        separated.push_bind(description.clone());
    }
    if let Some(date_time_original) = fields.date_time_original {
        separated.push(r#""dateTimeOriginal" = "#);
        separated.push_bind(date_time_original);
    }
    if let Some(time_zone) = &fields.time_zone {
        separated.push(r#""timeZone" = "#);
        separated.push_bind(time_zone.clone());
    }
    if let Some(latitude) = fields.latitude {
        separated.push("latitude = ");
        separated.push_bind(latitude);
    }
    if let Some(longitude) = fields.longitude {
        separated.push("longitude = ");
        separated.push_bind(longitude);
    }
    if let Some(rating) = &fields.rating {
        separated.push("rating = ");
        separated.push_bind(*rating);
    }

    query.push(r#" WHERE "assetId" = ANY("#);
    query.push_bind(asset_ids);
    query.push("::uuid[])");
    query.build().execute(pool).await?;

    let locked = fields.locked_property_names();
    if !locked.is_empty() {
        append_locked_properties(pool, asset_ids, &locked).await?;
    }
    Ok(())
}

pub async fn trash_assets(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
    force: bool,
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    let status = if force { "deleted" } else { "trashed" };
    sqlx::query(
        r#"
            UPDATE asset
            SET "deletedAt" = NOW(), status = $1
            WHERE id = ANY($2)
        "#,
    )
    .bind(status)
    .bind(asset_ids)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, FromRow)]
pub struct AssetCopyRow {
    pub id: Uuid,
    pub stack_id: Option<Uuid>,
    pub original_path: String,
    pub is_favorite: bool,
}

pub async fn get_for_copy(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<AssetCopyRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetCopyRow>(
        r#"
            SELECT
                id,
                "stackId" as stack_id,
                "originalPath" as original_path,
                "isFavorite" as is_favorite
            FROM asset
            WHERE id = $1 AND "deletedAt" IS NULL
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn get_asset_file_path(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    file_type: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT path
            FROM asset_file
            WHERE "assetId" = $1 AND type = $2 AND "isEdited" = false
            LIMIT 1
        "#,
    )
    .bind(asset_id)
    .bind(file_type)
    .fetch_optional(pool)
    .await
}

pub async fn upsert_sidecar_file(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            INSERT INTO asset_file ("assetId", type, path)
            VALUES ($1, 'sidecar', $2)
            ON CONFLICT ("assetId", type, "isEdited")
            DO UPDATE SET path = EXCLUDED.path, "updatedAt" = NOW()
        "#,
    )
    .bind(asset_id)
    .bind(path)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn copy_album_associations(
    pool: &Pool<Postgres>,
    source_asset_id: &Uuid,
    target_asset_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            INSERT INTO album_asset ("albumId", "assetId")
            SELECT aa."albumId", $2
            FROM album_asset aa
            WHERE aa."assetId" = $1
            ON CONFLICT DO NOTHING
        "#,
    )
    .bind(source_asset_id)
    .bind(target_asset_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn copy_shared_link_associations(
    pool: &Pool<Postgres>,
    source_asset_id: &Uuid,
    target_asset_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            INSERT INTO shared_link_asset ("assetId", "sharedLinkId")
            SELECT $2, sla."sharedLinkId"
            FROM shared_link_asset sla
            WHERE sla."assetId" = $1
            ON CONFLICT DO NOTHING
        "#,
    )
    .bind(source_asset_id)
    .bind(target_asset_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_stack_id(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    stack_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE asset
            SET "stackId" = $2, "updatedAt" = NOW()
            WHERE id = $1
        "#,
    )
    .bind(asset_id)
    .bind(stack_id)
    .execute(pool)
    .await?;
    Ok(())
}
