use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::db::DbService;
use crate::utils::permission::require_permission;
use crate::models::db::auth_permission::Permission;

#[derive(Clone)]
pub struct AlbumService {
    db: DbService,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AlbumResponse {
    pub id: Uuid,
    pub album_name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub owner_id: Uuid,
    pub album_thumbnail_asset_id: Option<Uuid>,
    pub is_activity_enabled: bool,
    pub order: String,
    #[serde(default)]
    pub asset_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumStatisticsResponse {
    pub owned: i64,
    pub shared: i64,
    pub not_shared: i64,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumsQuery {
    pub shared: Option<bool>,
    pub asset_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAlbumReq {
    pub album_name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub asset_ids: Vec<Uuid>,
}

impl AlbumService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            db: DbService::new(pool),
        }
    }

    pub async fn get_statistics(&self, auth: &AuthDto) -> Result<AlbumStatisticsResponse, ErrorResp> {
        require_permission(auth, Permission::AlbumStatistics)?;

        let owned: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM album WHERE "ownerId" = $1"#,
        )
        .bind(auth.user.id)
        .fetch_one(&self.db.pool)
        .await?;

        let shared: i64 = sqlx::query_scalar(
            r#"
                SELECT COUNT(DISTINCT a.id)
                FROM album a
                INNER JOIN album_user au ON au."albumId" = a.id
                WHERE au."userId" = $1 AND a."ownerId" != $1
            "#,
        )
        .bind(auth.user.id)
        .fetch_one(&self.db.pool)
        .await?;

        Ok(AlbumStatisticsResponse {
            owned,
            shared,
            not_shared: owned - shared.max(0),
        })
    }

    pub async fn get_all(
        &self,
        auth: &AuthDto,
        query: &GetAlbumsQuery,
    ) -> Result<Vec<AlbumResponse>, ErrorResp> {
        require_permission(auth, Permission::AlbumRead)?;

        let albums = if query.shared == Some(true) {
            sqlx::query_as::<_, AlbumResponse>(
                r#"
                    SELECT a.id, a."albumName" as album_name, a.description,
                           a."createdAt" as created_at, a."updatedAt" as updated_at,
                           a."ownerId" as owner_id,
                           a."albumThumbnailAssetId" as album_thumbnail_asset_id,
                           a."isActivityEnabled" as is_activity_enabled,
                           a."order" as "order",
                           COALESCE((SELECT COUNT(*) FROM album_asset aa WHERE aa."albumId" = a.id), 0) as asset_count
                    FROM album a
                    INNER JOIN album_user au ON au."albumId" = a.id
                    WHERE au."userId" = $1 AND a."ownerId" != $1
                    ORDER BY a."updatedAt" DESC
                "#,
            )
            .bind(auth.user.id)
            .fetch_all(&self.db.pool)
            .await?
        } else {
            sqlx::query_as::<_, AlbumResponse>(
                r#"
                    SELECT a.id, a."albumName" as album_name, a.description,
                           a."createdAt" as created_at, a."updatedAt" as updated_at,
                           a."ownerId" as owner_id,
                           a."albumThumbnailAssetId" as album_thumbnail_asset_id,
                           a."isActivityEnabled" as is_activity_enabled,
                           a."order" as "order",
                           COALESCE((SELECT COUNT(*) FROM album_asset aa WHERE aa."albumId" = a.id), 0) as asset_count
                    FROM album a
                    INNER JOIN album_user au ON au."albumId" = a.id
                    WHERE au."userId" = $1
                    ORDER BY a."updatedAt" DESC
                "#,
            )
            .bind(auth.user.id)
            .fetch_all(&self.db.pool)
            .await?
        };

        Ok(albums)
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<AlbumResponse, ErrorResp> {
        require_permission(auth, Permission::AlbumRead)?;
        self.get_accessible(auth, id).await
    }

    pub async fn create(&self, auth: &AuthDto, dto: &CreateAlbumReq) -> Result<AlbumResponse, ErrorResp> {
        require_permission(auth, Permission::AlbumCreate)?;

        let mut tx = self.db.pool.begin().await?;

        let album = sqlx::query_as::<_, AlbumResponse>(
            r#"
                INSERT INTO album ("albumName", description, "ownerId")
                VALUES ($1, $2, $3)
                RETURNING id, "albumName" as album_name, description,
                          "createdAt" as created_at, "updatedAt" as updated_at,
                          "ownerId" as owner_id,
                          "albumThumbnailAssetId" as album_thumbnail_asset_id,
                          "isActivityEnabled" as is_activity_enabled,
                          "order" as "order",
                          0::bigint as asset_count
            "#,
        )
        .bind(&dto.album_name)
        .bind(&dto.description)
        .bind(auth.user.id)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"INSERT INTO album_user ("albumId", "userId", role) VALUES ($1, $2, 'owner')"#,
        )
        .bind(album.id)
        .bind(auth.user.id)
        .execute(&mut *tx)
        .await?;

        for asset_id in &dto.asset_ids {
            sqlx::query(
                r#"INSERT INTO album_asset ("albumId", "assetId") VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
            )
            .bind(album.id)
            .bind(asset_id)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        self.get(auth, &album.id).await
    }

    async fn get_accessible(&self, auth: &AuthDto, id: &Uuid) -> Result<AlbumResponse, ErrorResp> {
        sqlx::query_as::<_, AlbumResponse>(
            r#"
                SELECT a.id, a."albumName" as album_name, a.description,
                       a."createdAt" as created_at, a."updatedAt" as updated_at,
                       a."ownerId" as owner_id,
                       a."albumThumbnailAssetId" as album_thumbnail_asset_id,
                       a."isActivityEnabled" as is_activity_enabled,
                       a."order" as "order",
                       COALESCE((SELECT COUNT(*) FROM album_asset aa WHERE aa."albumId" = a.id), 0) as asset_count
                FROM album a
                INNER JOIN album_user au ON au."albumId" = a.id
                WHERE a.id = $1 AND au."userId" = $2
            "#,
        )
        .bind(id)
        .bind(auth.user.id)
        .fetch_optional(&self.db.pool)
        .await?
        .ok_or_else(|| ErrorResp::BadRequest("Not found or no album.read access".to_string()))
    }
}
