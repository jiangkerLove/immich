use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumUserRole {
    Owner,
    Editor,
    Viewer,
}

impl AlbumUserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumAccessLevel {
    Owner,
    Editor,
    Member,
}

pub async fn has_album_access(
    pool: &PgPool,
    user_id: &Uuid,
    album_id: &Uuid,
    level: AlbumAccessLevel,
) -> Result<bool, sqlx::Error> {
    let roles: &[&str] = match level {
        AlbumAccessLevel::Owner => &["owner"],
        AlbumAccessLevel::Editor => &["owner", "editor"],
        AlbumAccessLevel::Member => &["owner", "editor", "viewer"],
    };

    let exists: bool = sqlx::query_scalar(
        r#"
            SELECT EXISTS(
                SELECT 1
                FROM album a
                INNER JOIN album_user au ON au."albumId" = a.id
                WHERE a.id = $1
                  AND a."deletedAt" IS NULL
                  AND au."userId" = $2
                  AND au.role = ANY($3)
            )
        "#,
    )
    .bind(album_id)
    .bind(user_id)
    .bind(roles)
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

pub async fn shared_link_has_album(
    pool: &PgPool,
    shared_link_id: &Uuid,
    album_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let exists: bool = sqlx::query_scalar(
        r#"
            SELECT EXISTS(
                SELECT 1
                FROM shared_link
                WHERE id = $1
                  AND "albumId" = $2
            )
        "#,
    )
    .bind(shared_link_id)
    .bind(album_id)
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

pub async fn get_album_thumbnail_asset_id(
    pool: &PgPool,
    album_id: &Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(r#"SELECT "albumThumbnailAssetId" FROM album WHERE id = $1"#)
        .bind(album_id)
        .fetch_optional(pool)
        .await
}

pub async fn filter_asset_ids_in_album(
    pool: &PgPool,
    album_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<HashSet<Uuid>, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(HashSet::new());
    }

    let rows: Vec<Uuid> = sqlx::query_scalar(
        r#"
            SELECT "assetId"
            FROM album_asset
            WHERE "albumId" = $1 AND "assetId" = ANY($2)
        "#,
    )
    .bind(album_id)
    .bind(asset_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().collect())
}

pub async fn add_asset_ids(
    pool: &PgPool,
    album_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    for asset_id in asset_ids {
        sqlx::query(
            r#"
                INSERT INTO album_asset ("albumId", "assetId")
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
            "#,
        )
        .bind(album_id)
        .bind(asset_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn remove_asset_ids(
    pool: &PgPool,
    album_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"
            DELETE FROM album_asset
            WHERE "albumId" = $1 AND "assetId" = ANY($2)
        "#,
    )
    .bind(album_id)
    .bind(asset_ids)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn set_album_thumbnail(
    pool: &PgPool,
    album_id: &Uuid,
    asset_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE album
            SET "albumThumbnailAssetId" = $1
            WHERE id = $2
        "#,
    )
    .bind(asset_id)
    .bind(album_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_album_thumbnails(pool: &PgPool, album_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE album
            SET "albumThumbnailAssetId" = (
                SELECT aa."assetId"
                FROM album_asset aa
                INNER JOIN asset ON asset.id = aa."assetId" AND asset."deletedAt" IS NULL
                WHERE aa."albumId" = album.id
                ORDER BY asset."fileCreatedAt" DESC
                LIMIT 1
            )
            WHERE id = $1
              AND (
                ("albumThumbnailAssetId" IS NULL AND EXISTS (
                    SELECT 1 FROM album_asset aa
                    INNER JOIN asset ON asset.id = aa."assetId" AND asset."deletedAt" IS NULL
                    WHERE aa."albumId" = album.id
                ))
                OR (
                    "albumThumbnailAssetId" IS NOT NULL
                    AND NOT EXISTS (
                        SELECT 1 FROM album_asset aa
                        WHERE aa."albumId" = album.id
                          AND aa."assetId" = album."albumThumbnailAssetId"
                    )
                )
              )
        "#,
    )
    .bind(album_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_all_album_thumbnails(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
            UPDATE album
            SET "albumThumbnailAssetId" = (
                SELECT aa."assetId"
                FROM album_asset aa
                INNER JOIN asset ON asset.id = aa."assetId" AND asset."deletedAt" IS NULL
                WHERE aa."albumId" = album.id
                ORDER BY asset."fileCreatedAt" DESC
                LIMIT 1
            )
            WHERE (
                ("albumThumbnailAssetId" IS NULL AND EXISTS (
                    SELECT 1 FROM album_asset aa
                    INNER JOIN asset ON asset.id = aa."assetId" AND asset."deletedAt" IS NULL
                    WHERE aa."albumId" = album.id
                ))
                OR (
                    "albumThumbnailAssetId" IS NOT NULL
                    AND NOT EXISTS (
                        SELECT 1 FROM album_asset aa
                        WHERE aa."albumId" = album.id
                          AND aa."assetId" = album."albumThumbnailAssetId"
                    )
                )
            )
              AND album."deletedAt" IS NULL
        "#,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn touch_album_updated_at(pool: &PgPool, album_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE album
            SET "updatedAt" = now()
            WHERE id = $1
        "#,
    )
    .bind(album_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn user_exists(pool: &PgPool, user_id: &Uuid) -> Result<bool, sqlx::Error> {
    let exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "user" WHERE id = $1 AND "deletedAt" IS NULL)"#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

pub async fn album_user_exists(
    pool: &PgPool,
    album_id: &Uuid,
    user_id: &Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT role::text FROM album_user WHERE "albumId" = $1 AND "userId" = $2"#,
    )
    .bind(album_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn count_album_owners(pool: &PgPool, album_id: &Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM album_user WHERE "albumId" = $1 AND role = 'owner'"#,
    )
    .bind(album_id)
    .fetch_one(pool)
    .await
}

pub async fn add_album_user(
    pool: &PgPool,
    album_id: &Uuid,
    user_id: &Uuid,
    role: AlbumUserRole,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            INSERT INTO album_user ("albumId", "userId", role)
            VALUES ($1, $2, $3::album_user_role_enum)
        "#,
    )
    .bind(album_id)
    .bind(user_id)
    .bind(role.as_str())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_album_user_role(
    pool: &PgPool,
    album_id: &Uuid,
    user_id: &Uuid,
    role: AlbumUserRole,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE album_user
            SET role = $3::album_user_role_enum
            WHERE "albumId" = $1 AND "userId" = $2
        "#,
    )
    .bind(album_id)
    .bind(user_id)
    .bind(role.as_str())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_album_user(
    pool: &PgPool,
    album_id: &Uuid,
    user_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"DELETE FROM album_user WHERE "albumId" = $1 AND "userId" = $2"#,
    )
    .bind(album_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_album(pool: &PgPool, album_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM album WHERE id = $1"#)
        .bind(album_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub fn parse_album_user_role(role: &str) -> Option<AlbumUserRole> {
    match role {
        "editor" => Some(AlbumUserRole::Editor),
        "owner" => Some(AlbumUserRole::Owner),
        "viewer" => Some(AlbumUserRole::Viewer),
        _ => None,
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct AlbumRow {
    pub id: Uuid,
    pub album_name: String,
    pub description: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub album_thumbnail_asset_id: Option<Uuid>,
    pub is_activity_enabled: bool,
    pub order: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct AlbumUserRow {
    pub user_id: Uuid,
    pub role: String,
    pub name: String,
    pub email: String,
    pub profile_image_path: String,
    pub avatar_color: Option<String>,
    pub profile_changed_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_album_member_ids(
    pool: &PgPool,
    album_id: &Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(r#"SELECT "userId" FROM album_user WHERE "albumId" = $1"#)
        .bind(album_id)
        .fetch_all(pool)
        .await
}

pub async fn get_album_row(
    pool: &PgPool,
    album_id: &Uuid,
) -> Result<Option<AlbumRow>, sqlx::Error> {
    sqlx::query_as::<_, AlbumRow>(
        r#"
            SELECT id,
                   "albumName" as album_name,
                   description,
                   "createdAt" as created_at,
                   "updatedAt" as updated_at,
                   "albumThumbnailAssetId" as album_thumbnail_asset_id,
                   "isActivityEnabled" as is_activity_enabled,
                   "order" as "order"
            FROM album
            WHERE id = $1 AND "deletedAt" IS NULL
        "#,
    )
    .bind(album_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_album_users(
    pool: &PgPool,
    album_id: &Uuid,
    auth_user_id: &Uuid,
) -> Result<Vec<AlbumUserRow>, sqlx::Error> {
    sqlx::query_as::<_, AlbumUserRow>(
        r#"
            SELECT u.id as user_id,
                   au.role::text as role,
                   u.name,
                   u.email,
                   u."profileImagePath" as profile_image_path,
                   u."avatarColor" as avatar_color,
                   u."profileChangedAt" as profile_changed_at
            FROM album_user au
            INNER JOIN "user" u ON u.id = au."userId"
            WHERE au."albumId" = $1
            ORDER BY au.role,
                     CASE WHEN au."userId" = $2 THEN 0 ELSE 1 END,
                     u.name ASC
        "#,
    )
    .bind(album_id)
    .bind(auth_user_id)
    .fetch_all(pool)
    .await
}

pub async fn album_has_shared_link(pool: &PgPool, album_id: &Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM shared_link WHERE "albumId" = $1)"#,
    )
    .bind(album_id)
    .fetch_one(pool)
    .await
}

pub async fn count_album_assets(pool: &PgPool, album_id: &Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM album_asset WHERE "albumId" = $1"#)
        .bind(album_id)
        .fetch_one(pool)
        .await
}

pub async fn count_owned_albums(pool: &PgPool, user_id: &Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT COUNT(*)
            FROM album a
            INNER JOIN album_user au ON au."albumId" = a.id
            WHERE au."userId" = $1
              AND au.role = 'owner'
              AND a."deletedAt" IS NULL
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn count_shared_albums(pool: &PgPool, user_id: &Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT COUNT(*)
            FROM album a
            INNER JOIN album_user au ON au."albumId" = a.id
            WHERE au."userId" = $1
              AND a."deletedAt" IS NULL
              AND (
                EXISTS (
                  SELECT 1 FROM album_user au2
                  WHERE au2."albumId" = a.id AND au2.role != 'owner'
                )
                OR EXISTS (
                  SELECT 1 FROM shared_link sl WHERE sl."albumId" = a.id
                )
              )
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn count_owned_not_shared_albums(pool: &PgPool, user_id: &Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT COUNT(*)
            FROM album a
            INNER JOIN album_user au ON au."albumId" = a.id
            WHERE au."userId" = $1
              AND au.role = 'owner'
              AND a."deletedAt" IS NULL
              AND NOT EXISTS (
                SELECT 1 FROM album_user au2
                WHERE au2."albumId" = a.id AND au2.role != 'owner'
              )
              AND NOT EXISTS (
                SELECT 1 FROM shared_link sl WHERE sl."albumId" = a.id
              )
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn list_accessible_album_ids(
    pool: &PgPool,
    user_id: &Uuid,
    is_owned: Option<bool>,
    is_shared: Option<bool>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut query = String::from(
        r#"
            SELECT a.id
            FROM album a
            INNER JOIN album_user au ON au."albumId" = a.id
            WHERE au."userId" = $1
              AND a."deletedAt" IS NULL
        "#,
    );

    if is_owned == Some(true) {
        query.push_str(" AND au.role = 'owner'");
    } else if is_owned == Some(false) {
        query.push_str(" AND au.role != 'owner'");
    }

    if is_shared == Some(true) {
        query.push_str(
            r#" AND (
                EXISTS (SELECT 1 FROM album_user au2 WHERE au2."albumId" = a.id AND au2.role != 'owner')
                OR EXISTS (SELECT 1 FROM shared_link sl WHERE sl."albumId" = a.id)
            )"#,
        );
    } else if is_shared == Some(false) {
        query.push_str(
            r#" AND NOT EXISTS (
                SELECT 1 FROM album_user au2 WHERE au2."albumId" = a.id AND au2.role != 'owner'
            ) AND NOT EXISTS (
                SELECT 1 FROM shared_link sl WHERE sl."albumId" = a.id
            )"#,
        );
    }

    query.push_str(r#" ORDER BY a."createdAt" DESC"#);

    sqlx::query_scalar::<_, Uuid>(&query)
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub async fn list_album_ids_by_asset(
    pool: &PgPool,
    user_id: &Uuid,
    asset_id: &Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT a.id
            FROM album a
            INNER JOIN album_user au ON au."albumId" = a.id
            INNER JOIN album_asset aa ON aa."albumId" = a.id
            WHERE au."userId" = $1
              AND aa."assetId" = $2
              AND a."deletedAt" IS NULL
            ORDER BY a."createdAt" DESC
        "#,
    )
    .bind(user_id)
    .bind(asset_id)
    .fetch_all(pool)
    .await
}

#[derive(Debug, FromRow)]
pub struct AlbumMetadataRow {
    pub album_id: Uuid,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub last_modified_asset_timestamp: Option<DateTime<Utc>>,
    pub asset_count: i64,
}

#[derive(Debug, FromRow)]
pub struct ContributorCountRow {
    pub user_id: Uuid,
    pub asset_count: i64,
}

pub async fn get_metadata_for_ids(
    pool: &PgPool,
    album_ids: &[Uuid],
) -> Result<Vec<AlbumMetadataRow>, sqlx::Error> {
    if album_ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_as::<_, AlbumMetadataRow>(
        r#"
            SELECT
                aa."albumId" as album_id,
                MIN(a."localDateTime") as start_date,
                MAX(a."localDateTime") as end_date,
                MAX(a."updatedAt") as last_modified_asset_timestamp,
                COUNT(a.id)::bigint as asset_count
            FROM album_asset aa
            INNER JOIN asset a ON a.id = aa."assetId"
            WHERE aa."albumId" = ANY($1)
              AND a."deletedAt" IS NULL
              AND a.visibility != 'hidden'
            GROUP BY aa."albumId"
        "#,
    )
    .bind(album_ids)
    .fetch_all(pool)
    .await
}

pub async fn get_contributor_counts(
    pool: &PgPool,
    album_id: &Uuid,
) -> Result<Vec<ContributorCountRow>, sqlx::Error> {
    sqlx::query_as::<_, ContributorCountRow>(
        r#"
            SELECT
                a."ownerId" as user_id,
                COUNT(*)::bigint as asset_count
            FROM album_asset aa
            INNER JOIN asset a ON a.id = aa."assetId"
            WHERE aa."albumId" = $1
              AND a."deletedAt" IS NULL
            GROUP BY a."ownerId"
            ORDER BY asset_count DESC
        "#,
    )
    .bind(album_id)
    .fetch_all(pool)
    .await
}
