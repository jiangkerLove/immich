use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::assets::{self};
use crate::models::db::auth_permission::Permission;
use crate::models::db::shared_links::{
    self, NewSharedLink, SharedLinkRow, SharedLinkSearch, UpdateSharedLink,
};
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::models::response::shared_link::{
    map_shared_link, SharedLinkAlbumResponse, SharedLinkResponse,
};
use crate::service::album::AlbumResponse;
use crate::utils::crypto::{random_bytes, shared_link_login_token};
use crate::utils::permission::require_permission;

#[derive(Clone)]
pub struct SharedLinkService {
    pool: PgPool,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedLinkSearchQuery {
    pub id: Option<Uuid>,
    pub album_id: Option<Uuid>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedLinkCreateReq {
    #[serde(rename = "type")]
    pub link_type: String,
    pub asset_ids: Option<Vec<Uuid>>,
    pub album_id: Option<Uuid>,
    pub description: Option<String>,
    pub password: Option<String>,
    pub slug: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub allow_upload: Option<bool>,
    pub allow_download: Option<bool>,
    pub show_metadata: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedLinkEditReq {
    #[serde(default, deserialize_with = "crate::utils::serde::deserialize_patch_option")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::utils::serde::deserialize_patch_option")]
    pub password: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::utils::serde::deserialize_patch_option")]
    pub slug: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::utils::serde::deserialize_patch_option")]
    pub expires_at: Option<Option<DateTime<Utc>>>,
    pub allow_upload: Option<bool>,
    pub allow_download: Option<bool>,
    pub show_metadata: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
pub struct SharedLinkLoginReq {
    pub password: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIdsReq {
    pub asset_ids: Vec<Uuid>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIdsResponse {
    pub asset_id: Uuid,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SharedLinkService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_all(
        &self,
        auth: &AuthDto,
        query: &SharedLinkSearchQuery,
    ) -> Result<Vec<SharedLinkResponse>, ErrorResp> {
        require_permission(auth, Permission::SharedLinkRead)?;
        let rows = shared_links::list_for_user(
            &self.pool,
            &auth.user.id,
            &SharedLinkSearch {
                id: query.id,
                album_id: query.album_id,
            },
        )
        .await?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            results.push(self.build_response(&row, auth, false, Some(1)).await?);
        }
        Ok(results)
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<SharedLinkResponse, ErrorResp> {
        require_permission(auth, Permission::SharedLinkRead)?;
        let row = self.find_or_fail(&auth.user.id, id).await?;
        self.build_response(&row, auth, false, None).await
    }

    pub async fn get_mine(
        &self,
        auth: &AuthDto,
        _auth_tokens: &[String],
    ) -> Result<SharedLinkResponse, ErrorResp> {
        let shared_link = auth
            .shared_link
            .as_ref()
            .ok_or_else(|| ErrorResp::Forbidden("Forbidden".to_string()))?;

        let link_id = Uuid::parse_str(&shared_link.id)
            .map_err(|_| ErrorResp::ServerError("Invalid shared link".to_string()))?;
        let row = self
            .find_or_fail(&auth.user.id, &link_id)
            .await?;

        self.build_response(&row, auth, !row.show_exif, None).await
    }

    pub async fn login(
        &self,
        auth: &AuthDto,
        dto: &SharedLinkLoginReq,
    ) -> Result<(SharedLinkResponse, String), ErrorResp> {
        let shared_link = auth
            .shared_link
            .as_ref()
            .ok_or_else(|| ErrorResp::Forbidden("Forbidden".to_string()))?;

        let link_id = Uuid::parse_str(&shared_link.id)
            .map_err(|_| ErrorResp::ServerError("Invalid shared link".to_string()))?;
        let row = self
            .find_or_fail(&auth.user.id, &link_id)
            .await?;

        let password = row
            .password
            .as_deref()
            .ok_or_else(|| ErrorResp::BadRequest("Shared link is not password protected".to_string()))?;

        if password != dto.password {
            return Err(ErrorResp::Unauthorized("Invalid password".to_string()));
        }

        let token = shared_link_login_token(&row.id, password);
        let response = self.build_response(&row, auth, !row.show_exif, None).await?;
        Ok((response, token))
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &SharedLinkCreateReq,
    ) -> Result<SharedLinkResponse, ErrorResp> {
        require_permission(auth, Permission::SharedLinkCreate)?;

        match dto.link_type.as_str() {
            "ALBUM" => {
                let album_id = dto
                    .album_id
                    .ok_or_else(|| ErrorResp::BadRequest("Invalid albumId".to_string()))?;
                require_permission(auth, Permission::AlbumShare)?;
                if !shared_links::user_owns_album(&self.pool, &auth.user.id, &album_id).await? {
                    return Err(ErrorResp::BadRequest(
                        "Not found or no album.share access".to_string(),
                    ));
                }
            }
            "INDIVIDUAL" => {
                let asset_ids = dto
                    .asset_ids
                    .as_ref()
                    .filter(|ids| !ids.is_empty())
                    .ok_or_else(|| ErrorResp::BadRequest("Invalid assetIds".to_string()))?;
                self.require_asset_share(auth, asset_ids).await?;
            }
            _ => {
                return Err(ErrorResp::BadRequest("Invalid shared link type".to_string()));
            }
        }

        let show_metadata = dto.show_metadata.unwrap_or(true);
        let allow_download = if show_metadata {
            dto.allow_download.unwrap_or(true)
        } else {
            false
        };

        let row = shared_links::create(
            &self.pool,
            NewSharedLink {
                user_id: auth.user.id,
                key: &random_bytes(50),
                link_type: &dto.link_type,
                album_id: dto.album_id,
                description: dto.description.as_deref(),
                password: dto.password.as_deref(),
                slug: dto.slug.as_deref(),
                expires_at: dto.expires_at,
                allow_upload: dto.allow_upload.unwrap_or(true),
                allow_download,
                show_exif: show_metadata,
                asset_ids: dto.asset_ids.as_deref().unwrap_or(&[]),
            },
        )
        .await
        .map_err(map_shared_link_error)?;

        self.build_response(&row, auth, false, None).await
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &SharedLinkEditReq,
    ) -> Result<SharedLinkResponse, ErrorResp> {
        require_permission(auth, Permission::SharedLinkUpdate)?;
        self.find_or_fail(&auth.user.id, id).await?;

        let row = shared_links::update(
            &self.pool,
            &auth.user.id,
            id,
            UpdateSharedLink {
                description: dto
                    .description
                    .as_ref()
                    .map(|value| value.as_deref()),
                password: dto.password.as_ref().map(|value| value.as_deref()),
                slug: dto.slug.as_ref().map(|value| value.as_deref()),
                expires_at: dto.expires_at,
                allow_upload: dto.allow_upload,
                allow_download: dto.allow_download,
                show_exif: dto.show_metadata,
                asset_ids: None,
            },
        )
        .await
        .map_err(map_shared_link_error)?;

        self.build_response(&row, auth, false, None).await
    }

    pub async fn remove(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::SharedLinkDelete)?;
        let row = self.find_or_fail(&auth.user.id, id).await?;
        shared_links::remove(&self.pool, &row.id).await?;
        Ok(())
    }

    pub async fn add_assets(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &AssetIdsReq,
    ) -> Result<Vec<AssetIdsResponse>, ErrorResp> {
        require_permission(auth, Permission::SharedLinkUpdate)?;
        let row = self.find_or_fail(&auth.user.id, id).await?;

        if row.link_type != "INDIVIDUAL" {
            return Err(ErrorResp::BadRequest("Invalid shared link type".to_string()));
        }

        let existing = shared_links::list_asset_ids(&self.pool, &row.id, None).await?;
        let existing_set: std::collections::HashSet<_> = existing.into_iter().collect();

        let new_ids: Vec<Uuid> = dto
            .asset_ids
            .iter()
            .filter(|id| !existing_set.contains(*id))
            .copied()
            .collect();
        let allowed = assets::filter_asset_share_ids(&self.pool, &auth.user.id, &new_ids).await?;
        let allowed_set: std::collections::HashSet<_> = allowed.into_iter().collect();

        let mut to_add = Vec::new();
        let mut results = Vec::new();

        for asset_id in &dto.asset_ids {
            if existing_set.contains(asset_id) {
                results.push(AssetIdsResponse {
                    asset_id: *asset_id,
                    success: false,
                    error: Some("duplicate".to_string()),
                });
                continue;
            }
            if !allowed_set.contains(asset_id) {
                results.push(AssetIdsResponse {
                    asset_id: *asset_id,
                    success: false,
                    error: Some("no_permission".to_string()),
                });
                continue;
            }
            to_add.push(*asset_id);
            results.push(AssetIdsResponse {
                asset_id: *asset_id,
                success: true,
                error: None,
            });
        }

        if !to_add.is_empty() {
            shared_links::add_assets(&self.pool, &row.id, &to_add).await?;
        }

        Ok(results)
    }

    pub async fn remove_assets(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &AssetIdsReq,
    ) -> Result<Vec<AssetIdsResponse>, ErrorResp> {
        require_permission(auth, Permission::SharedLinkUpdate)?;
        let row = self.find_or_fail(&auth.user.id, id).await?;

        if row.link_type != "INDIVIDUAL" {
            return Err(ErrorResp::BadRequest("Invalid shared link type".to_string()));
        }

        let removed = shared_links::remove_assets(&self.pool, &row.id, &dto.asset_ids).await?;
        let removed_set: std::collections::HashSet<_> = removed.into_iter().collect();

        Ok(dto
            .asset_ids
            .iter()
            .map(|asset_id| AssetIdsResponse {
                asset_id: *asset_id,
                success: removed_set.contains(asset_id),
                error: if removed_set.contains(asset_id) {
                    None
                } else {
                    Some("not_found".to_string())
                },
            })
            .collect())
    }

    async fn find_or_fail(&self, user_id: &Uuid, id: &Uuid) -> Result<SharedLinkRow, ErrorResp> {
        shared_links::get_for_user(&self.pool, user_id, id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Shared link not found".to_string()))
    }

    async fn build_response(
        &self,
        row: &SharedLinkRow,
        auth: &AuthDto,
        strip_asset_metadata: bool,
        asset_limit: Option<i64>,
    ) -> Result<SharedLinkResponse, ErrorResp> {
        let asset_ids = shared_links::list_asset_ids(&self.pool, &row.id, asset_limit).await?;
        let assets = assets::get_details_by_ids(&self.pool, &asset_ids).await?;

        let album = if let Some(album_id) = row.album_id {
            Some(self.load_album(&album_id).await?)
        } else {
            None
        };

        Ok(map_shared_link(row, &assets, album, auth, strip_asset_metadata))
    }

    async fn load_album(&self, album_id: &Uuid) -> Result<SharedLinkAlbumResponse, ErrorResp> {
        let album = sqlx::query_as::<_, AlbumResponse>(
            r#"
                SELECT a.id, a."albumName" as album_name, a.description,
                       a."createdAt" as created_at, a."updatedAt" as updated_at,
                       a."ownerId" as owner_id,
                       a."albumThumbnailAssetId" as album_thumbnail_asset_id,
                       a."isActivityEnabled" as is_activity_enabled,
                       a."order" as "order",
                       COALESCE((SELECT COUNT(*) FROM album_asset aa WHERE aa."albumId" = a.id), 0) as asset_count
                FROM album a
                WHERE a.id = $1 AND a."deletedAt" IS NULL
            "#,
        )
        .bind(album_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorResp::BadRequest("Album not found".to_string()))?;

        Ok(SharedLinkAlbumResponse {
            album,
            shared: true,
            has_shared_link: true,
        })
    }

    async fn require_asset_share(&self, auth: &AuthDto, asset_ids: &[Uuid]) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::AssetShare)?;
        let allowed = assets::filter_asset_share_ids(&self.pool, &auth.user.id, asset_ids).await?;
        if allowed.len() != asset_ids.len() {
            return Err(ErrorResp::BadRequest(
                "Not found or no asset.share access".to_string(),
            ));
        }
        Ok(())
    }
}

pub fn merge_shared_link_tokens(existing: &[String], token: &str) -> String {
    let mut tokens: Vec<String> = existing
        .iter()
        .filter(|value| !value.is_empty())
        .cloned()
        .collect();
    if !tokens.iter().any(|value| value == token) {
        tokens.push(token.to_string());
    }
    tokens.join(",")
}

fn map_shared_link_error(err: sqlx::Error) -> ErrorResp {
    if let sqlx::Error::Database(db_err) = &err {
        if db_err.constraint().is_some_and(|c| c.contains("shared_link_slug")) {
            return ErrorResp::BadRequest("Failed to save shared link".to_string());
        }
    }
    ErrorResp::from(err)
}
