use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::assets::{self};
use crate::models::db::auth_permission::Permission;
use crate::models::db::shared_links::{
    self, NewSharedLink, SharedLinkRow, SharedLinkSearch, UpdateSharedLink,
};
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::map_assets;
use crate::models::response::response::ErrorResp;
use crate::models::response::shared_link::{
    SharedLinkAlbumResponse, SharedLinkResponse, encode_key, map_shared_link,
};
use crate::service::access::require_album_access;
use crate::service::album::AlbumService;
use crate::utils::crypto::{random_bytes, shared_link_login_token};
use crate::utils::permission::require_permission;
use crate::utils::system_config::{get_merged, json_str};

#[derive(Clone)]
pub struct SharedLinkService {
    pool: PgPool,
    albums: AlbumService,
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
    #[serde(
        default,
        deserialize_with = "crate::utils::serde::deserialize_patch_option"
    )]
    pub description: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::utils::serde::deserialize_patch_option"
    )]
    pub password: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::utils::serde::deserialize_patch_option"
    )]
    pub slug: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::utils::serde::deserialize_patch_option"
    )]
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
    pub fn new(pool: PgPool, albums: AlbumService) -> Self {
        Self { pool, albums }
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
        let row = self.find_or_fail(&auth.user.id, &link_id).await?;

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
        let row = self.find_or_fail(&auth.user.id, &link_id).await?;

        let password = row.password.as_deref().ok_or_else(|| {
            ErrorResp::BadRequest("Shared link is not password protected".to_string())
        })?;

        if password != dto.password {
            return Err(ErrorResp::Unauthorized("Invalid password".to_string()));
        }

        let token = shared_link_login_token(&row.id, password);
        let response = self
            .build_response(&row, auth, !row.show_exif, None)
            .await?;
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
                if dto.asset_ids.as_ref().is_some_and(|ids| !ids.is_empty()) {
                    return Err(ErrorResp::BadRequest("Invalid assetIds".to_string()));
                }
                // Match TS Permission.AlbumShare: album owner or editor.
                require_album_access(&self.pool, auth, &album_id, Permission::AlbumShare).await?;
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
                return Err(ErrorResp::BadRequest(
                    "Invalid shared link type".to_string(),
                ));
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
                description: dto.description.as_ref().map(|value| value.as_deref()),
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
            return Err(ErrorResp::BadRequest(
                "Invalid shared link type".to_string(),
            ));
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
            return Err(ErrorResp::BadRequest(
                "Invalid shared link type".to_string(),
            ));
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

    pub async fn get_metadata_tags(
        &self,
        auth: &AuthDto,
        default_domain: Option<&str>,
    ) -> Result<Option<OpenGraphTags>, ErrorResp> {
        let shared_link = match auth.shared_link.as_ref() {
            Some(link) if link.password.is_none() => link,
            _ => return Ok(None),
        };

        let link_id = Uuid::parse_str(&shared_link.id)
            .map_err(|_| ErrorResp::ServerError("Invalid shared link".to_string()))?;
        let row = self.find_or_fail(&auth.user.id, &link_id).await?;

        let config = get_merged(&self.pool).await?;
        let external_domain = json_str(&config, &["server", "externalDomain"], "");
        let base = if !external_domain.is_empty() {
            external_domain
        } else {
            default_domain
                .unwrap_or("https://my.immich.app")
                .to_string()
        };

        let (album_name, album_thumbnail_id, album_asset_count) =
            if let Some(album_id) = row.album_id {
                let meta: Option<(String, Option<Uuid>, i64)> = sqlx::query_as(
                    r#"
                    SELECT
                        a."albumName",
                        a."albumThumbnailAssetId",
                        (
                            SELECT COUNT(*)::bigint
                            FROM album_asset aa
                            INNER JOIN asset asset ON asset.id = aa."assetId"
                            WHERE aa."albumId" = a.id AND asset."deletedAt" IS NULL
                        )
                    FROM album a
                    WHERE a.id = $1 AND a."deletedAt" IS NULL
                    "#,
                )
                .bind(album_id)
                .fetch_optional(&self.pool)
                .await?;
                match meta {
                    Some((name, thumb, count)) => (Some(name), thumb, count),
                    None => (None, None, 0),
                }
            } else {
                (None, None, 0)
            };

        let link_asset_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM shared_link_asset sla
            INNER JOIN asset a ON a.id = sla."assetId"
            WHERE sla."sharedLinkId" = $1 AND a."deletedAt" IS NULL
            "#,
        )
        .bind(row.id)
        .fetch_one(&self.pool)
        .await?;

        let first_asset_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT a.id
            FROM shared_link_asset sla
            INNER JOIN asset a ON a.id = sla."assetId"
            WHERE sla."sharedLinkId" = $1 AND a."deletedAt" IS NULL
            ORDER BY a."fileCreatedAt" ASC
            LIMIT 1
            "#,
        )
        .bind(row.id)
        .fetch_optional(&self.pool)
        .await?;

        let asset_count = if link_asset_count > 0 {
            link_asset_count
        } else {
            album_asset_count
        };

        let asset_id = album_thumbnail_id.or(first_asset_id);
        let key = encode_key(&row.key);
        let image_path = match asset_id {
            Some(id) => format!("/api/assets/{id}/thumbnail?key={key}"),
            None => "/feature-panel.png".to_string(),
        };

        let image_url = match url::Url::parse(&base) {
            Ok(base_url) => base_url
                .join(&image_path)
                .map(|url| url.to_string())
                .unwrap_or_else(|_| format!("{base}{image_path}")),
            Err(_) => format!("{base}{image_path}"),
        };

        Ok(Some(OpenGraphTags {
            title: album_name.unwrap_or_else(|| "Public Share".to_string()),
            description: row
                .description
                .unwrap_or_else(|| format!("{asset_count} shared photos & videos")),
            image_url: Some(image_url),
        }))
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
        let asset_rows = assets::get_details_by_ids(&self.pool, &asset_ids).await?;
        let mapped_assets = map_assets(&self.pool, &asset_rows, auth, strip_asset_metadata).await?;

        let album = if let Some(album_id) = row.album_id {
            Some(self.load_album(auth, &album_id).await?)
        } else {
            None
        };

        Ok(map_shared_link(row, mapped_assets, album))
    }

    async fn load_album(
        &self,
        auth: &AuthDto,
        album_id: &Uuid,
    ) -> Result<SharedLinkAlbumResponse, ErrorResp> {
        let album = self.albums.map_for_viewer(&auth.user.id, album_id).await?;

        Ok(SharedLinkAlbumResponse {
            album,
            shared: true,
            has_shared_link: true,
        })
    }

    async fn require_asset_share(
        &self,
        auth: &AuthDto,
        asset_ids: &[Uuid],
    ) -> Result<(), ErrorResp> {
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

#[derive(Debug, Clone)]
pub struct OpenGraphTags {
    pub title: String,
    pub description: String,
    pub image_url: Option<String>,
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
        if db_err
            .constraint()
            .is_some_and(|c| c.contains("shared_link_slug"))
        {
            return ErrorResp::BadRequest("Failed to save shared link".to_string());
        }
    }
    ErrorResp::from(err)
}
