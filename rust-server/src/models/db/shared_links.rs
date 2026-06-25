use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

use crate::models::db::users::AuthUserDb;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthSharedLinkDb {
    pub id: String,
    pub album_id: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub user_id: String,
    pub show_exif: bool,
    pub allow_upload: bool,
    pub allow_download: bool,
    pub password: Option<String>,
}

#[derive(Debug, FromRow)]
struct SharedLinkAuthRow {
    id: Uuid,
    user_id: Uuid,
    album_id: Option<Uuid>,
    expires_at: Option<DateTime<Utc>>,
    show_exif: bool,
    allow_upload: bool,
    allow_download: bool,
    password: Option<String>,
    auth_id: Uuid,
    auth_name: String,
    auth_email: String,
    auth_is_admin: bool,
    auth_quota_usage: i64,
    auth_quota_size: Option<i64>,
}

const SHARED_LINK_AUTH_QUERY: &str = r#"
    SELECT
        sl.id,
        sl."userId" as user_id,
        sl."albumId" as album_id,
        sl."expiresAt" as expires_at,
        sl."showExif" as show_exif,
        sl."allowUpload" as allow_upload,
        sl."allowDownload" as allow_download,
        sl.password,
        u.id as auth_id,
        u.name as auth_name,
        u.email as auth_email,
        u."isAdmin" as auth_is_admin,
        u."quotaUsageInBytes" as auth_quota_usage,
        u."quotaSizeInBytes" as auth_quota_size
    FROM shared_link sl
    LEFT JOIN album a ON a.id = sl."albumId" AND a."deletedAt" IS NULL
    INNER JOIN "user" u ON u.id = sl."userId" AND u."deletedAt" IS NULL
    WHERE (sl.type = 'INDIVIDUAL' OR a.id IS NOT NULL)
"#;

impl SharedLinkAuthRow {
    fn into_auth(self) -> (AuthUserDb, AuthSharedLinkDb) {
        let user = AuthUserDb {
            id: self.auth_id,
            is_admin: self.auth_is_admin,
            name: self.auth_name,
            email: self.auth_email,
            quota_usage_in_bytes: self.auth_quota_usage,
            quota_size_in_bytes: self.auth_quota_size,
        };
        let link = AuthSharedLinkDb {
            id: self.id.to_string(),
            album_id: self.album_id.map(|id| id.to_string()),
            expires_at: self.expires_at,
            user_id: self.user_id.to_string(),
            show_exif: self.show_exif,
            allow_upload: self.allow_upload,
            allow_download: self.allow_download,
            password: self.password,
        };
        (user, link)
    }

    fn is_valid(&self) -> bool {
        self.expires_at.is_none_or(|expires| expires > Utc::now())
    }
}

pub async fn get_by_key(
    pool: &Pool<Postgres>,
    key: &[u8],
) -> Result<Option<(AuthUserDb, AuthSharedLinkDb)>, sqlx::Error> {
    let query = format!(r#"{SHARED_LINK_AUTH_QUERY} AND sl.key = $1"#);
    let row = sqlx::query_as::<_, SharedLinkAuthRow>(&query)
        .bind(key)
        .fetch_optional(pool)
        .await?;

    Ok(row.filter(|r| r.is_valid()).map(SharedLinkAuthRow::into_auth))
}

pub async fn get_by_slug(
    pool: &Pool<Postgres>,
    slug: &str,
) -> Result<Option<(AuthUserDb, AuthSharedLinkDb)>, sqlx::Error> {
    let query = format!(r#"{SHARED_LINK_AUTH_QUERY} AND sl.slug = $1"#);
    let row = sqlx::query_as::<_, SharedLinkAuthRow>(&query)
        .bind(slug)
        .fetch_optional(pool)
        .await?;

    Ok(row.filter(|r| r.is_valid()).map(SharedLinkAuthRow::into_auth))
}

pub async fn add_assets(
    pool: &Pool<Postgres>,
    shared_link_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    for asset_id in asset_ids {
        sqlx::query(
            r#"INSERT INTO shared_link_asset ("sharedLinkId", "assetId") VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
        )
        .bind(shared_link_id)
        .bind(asset_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn add_album_assets(
    pool: &Pool<Postgres>,
    album_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    for asset_id in asset_ids {
        sqlx::query(
            r#"INSERT INTO album_asset ("albumId", "assetId") VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
        )
        .bind(album_id)
        .bind(asset_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SharedLinkRow {
    pub id: Uuid,
    pub description: Option<String>,
    pub user_id: Uuid,
    pub key: Vec<u8>,
    pub link_type: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub allow_upload: bool,
    pub album_id: Option<Uuid>,
    pub allow_download: bool,
    pub show_exif: bool,
    pub password: Option<String>,
    pub slug: Option<String>,
}

#[derive(Debug, Default)]
pub struct SharedLinkSearch {
    pub id: Option<Uuid>,
    pub album_id: Option<Uuid>,
}

#[derive(Debug)]
pub struct NewSharedLink<'a> {
    pub user_id: Uuid,
    pub key: &'a [u8],
    pub link_type: &'a str,
    pub album_id: Option<Uuid>,
    pub description: Option<&'a str>,
    pub password: Option<&'a str>,
    pub slug: Option<&'a str>,
    pub expires_at: Option<DateTime<Utc>>,
    pub allow_upload: bool,
    pub allow_download: bool,
    pub show_exif: bool,
    pub asset_ids: &'a [Uuid],
}

#[derive(Debug, Default)]
pub struct UpdateSharedLink<'a> {
    pub description: Option<Option<&'a str>>,
    pub password: Option<Option<&'a str>>,
    pub slug: Option<Option<&'a str>>,
    pub expires_at: Option<Option<DateTime<Utc>>>,
    pub allow_upload: Option<bool>,
    pub allow_download: Option<bool>,
    pub show_exif: Option<bool>,
    pub asset_ids: Option<&'a [Uuid]>,
}

const SHARED_LINK_SELECT: &str = r#"
    SELECT
        sl.id,
        sl.description,
        sl."userId" as user_id,
        sl.key,
        sl.type as link_type,
        sl."createdAt" as created_at,
        sl."expiresAt" as expires_at,
        sl."allowUpload" as allow_upload,
        sl."albumId" as album_id,
        sl."allowDownload" as allow_download,
        sl."showExif" as show_exif,
        sl.password,
        sl.slug
    FROM shared_link sl
    LEFT JOIN album a ON a.id = sl."albumId" AND a."deletedAt" IS NULL
"#;

const SHARED_LINK_VALID: &str =
    r#" WHERE sl."userId" = $1 AND (sl.type = 'INDIVIDUAL' OR a.id IS NOT NULL)"#;

pub async fn list_for_user(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    search: &SharedLinkSearch,
) -> Result<Vec<SharedLinkRow>, sqlx::Error> {
    let mut query = sqlx::QueryBuilder::new(SHARED_LINK_SELECT);
    query.push(SHARED_LINK_VALID);
    query.push_bind(user_id);

    if let Some(id) = search.id {
        query.push(r#" AND sl.id = "#);
        query.push_bind(id);
    }
    if let Some(album_id) = search.album_id {
        query.push(r#" AND sl."albumId" = "#);
        query.push_bind(album_id);
    }

    query.push(r#" ORDER BY sl."createdAt" DESC"#);
    query.build_query_as::<SharedLinkRow>().fetch_all(pool).await
}

pub async fn get_for_user(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    link_id: &Uuid,
) -> Result<Option<SharedLinkRow>, sqlx::Error> {
    let query = format!(r#"{SHARED_LINK_SELECT}{SHARED_LINK_VALID} AND sl.id = $2"#);
    sqlx::query_as::<_, SharedLinkRow>(&query)
        .bind(user_id)
        .bind(link_id)
        .fetch_optional(pool)
        .await
}

pub async fn create(pool: &Pool<Postgres>, link: NewSharedLink<'_>) -> Result<SharedLinkRow, sqlx::Error> {
    let id: Uuid = sqlx::query_scalar(
        r#"
            INSERT INTO shared_link (
                "userId", key, type, "albumId", description, password, slug,
                "expiresAt", "allowUpload", "allowDownload", "showExif"
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id
        "#,
    )
    .bind(link.user_id)
    .bind(link.key)
    .bind(link.link_type)
    .bind(link.album_id)
    .bind(link.description)
    .bind(link.password)
    .bind(link.slug)
    .bind(link.expires_at)
    .bind(link.allow_upload)
    .bind(link.allow_download)
    .bind(link.show_exif)
    .fetch_one(pool)
    .await?;

    if !link.asset_ids.is_empty() {
        add_assets(pool, &id, link.asset_ids).await?;
    }

    get_for_user(pool, &link.user_id, &id)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)
}

pub async fn update(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    link_id: &Uuid,
    fields: UpdateSharedLink<'_>,
) -> Result<SharedLinkRow, sqlx::Error> {
    let mut query = sqlx::QueryBuilder::new(r#"UPDATE shared_link SET "#);
    let mut separated = query.separated(", ");
    let mut has_update = false;

    if let Some(description) = fields.description {
        separated.push("description = ");
        separated.push_bind(description);
        has_update = true;
    }
    if let Some(password) = fields.password {
        separated.push("password = ");
        separated.push_bind(password);
        has_update = true;
    }
    if let Some(slug) = fields.slug {
        separated.push("slug = ");
        separated.push_bind(slug);
        has_update = true;
    }
    if let Some(expires_at) = fields.expires_at {
        separated.push(r#""expiresAt" = "#);
        separated.push_bind(expires_at);
        has_update = true;
    }
    if let Some(allow_upload) = fields.allow_upload {
        separated.push(r#""allowUpload" = "#);
        separated.push_bind(allow_upload);
        has_update = true;
    }
    if let Some(allow_download) = fields.allow_download {
        separated.push(r#""allowDownload" = "#);
        separated.push_bind(allow_download);
        has_update = true;
    }
    if let Some(show_exif) = fields.show_exif {
        separated.push(r#""showExif" = "#);
        separated.push_bind(show_exif);
        has_update = true;
    }

    if has_update {
        query.push(r#" WHERE id = "#);
        query.push_bind(link_id);
        query.build().execute(pool).await?;
    }

    if let Some(asset_ids) = fields.asset_ids {
        if !asset_ids.is_empty() {
            add_assets(pool, link_id, asset_ids).await?;
        }
    }

    get_for_user(pool, user_id, link_id)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)
}

pub async fn remove(pool: &Pool<Postgres>, link_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM shared_link WHERE id = $1"#)
        .bind(link_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_asset_ids(
    pool: &Pool<Postgres>,
    link_id: &Uuid,
    limit: Option<i64>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut query = sqlx::QueryBuilder::new(
        r#"
            SELECT asset.id
            FROM shared_link_asset
            INNER JOIN asset ON asset.id = shared_link_asset."assetId"
            WHERE shared_link_asset."sharedLinkId" =
        "#,
    );
    query.push_bind(link_id);
    query.push(r#" AND asset."deletedAt" IS NULL ORDER BY asset."fileCreatedAt" ASC"#);
    if let Some(limit) = limit {
        query.push(" LIMIT ");
        query.push_bind(limit);
    }

    query.build_query_scalar().fetch_all(pool).await
}

pub async fn remove_assets(
    pool: &Pool<Postgres>,
    link_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_scalar(
        r#"
            DELETE FROM shared_link_asset
            WHERE "sharedLinkId" = $1 AND "assetId" = ANY($2)
            RETURNING "assetId"
        "#,
    )
    .bind(link_id)
    .bind(asset_ids)
    .fetch_all(pool)
    .await
}

pub async fn user_owns_album(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    album_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT EXISTS(
                SELECT 1
                FROM album
                INNER JOIN album_user ON album_user."albumId" = album.id
                WHERE album.id = $1
                  AND album_user."userId" = $2
                  AND album_user.role = 'owner'
                  AND album."deletedAt" IS NULL
            )
        "#,
    )
    .bind(album_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
}
