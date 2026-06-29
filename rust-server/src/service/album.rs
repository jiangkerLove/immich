use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashSet;
use uuid::Uuid;

use crate::models::db::album::{self, AlbumAccessLevel, AlbumUserRole};
use crate::models::db::assets;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::access::{check_album_ids_access, require_album_access};
use crate::service::db::DbService;
use crate::service::job::JobService;
use crate::models::db::user_metadata::UserMetadataPO;
use crate::utils::permission::require_permission;
use crate::utils::preferences::resolve_preferences;
use crate::models::db::auth_permission::Permission;

#[derive(Clone)]
pub struct AlbumService {
    db: DbService,
    jobs: JobService,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumUserResponse {
    pub user: AlbumUserInfo,
    pub role: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumUserInfo {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub profile_image_path: String,
    pub avatar_color: String,
    pub profile_changed_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributorCountResponse {
    pub user_id: Uuid,
    pub asset_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumResponse {
    pub id: Uuid,
    pub album_name: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
    pub album_thumbnail_asset_id: Option<Uuid>,
    pub shared: bool,
    pub album_users: Vec<AlbumUserResponse>,
    pub has_shared_link: bool,
    pub asset_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified_asset_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    pub is_activity_enabled: bool,
    pub order: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributor_counts: Option<Vec<ContributorCountResponse>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumStatisticsResponse {
    pub owned: i64,
    pub shared: i64,
    pub not_shared: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkIdResponse {
    pub id: Uuid,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BulkIdErrorReason>,
}

#[derive(Debug, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum BulkIdErrorReason {
    Duplicate,
    NoPermission,
    NotFound,
    Validation,
    Unknown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumsAddAssetsResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BulkIdErrorReason>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumsQuery {
    pub id: Option<Uuid>,
    pub name: Option<String>,
    pub is_owned: Option<bool>,
    pub is_shared: Option<bool>,
    pub asset_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAlbumUserReq {
    pub user_id: Uuid,
    pub role: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAlbumReq {
    pub album_name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub asset_ids: Vec<Uuid>,
    #[serde(default)]
    pub album_users: Vec<CreateAlbumUserReq>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAlbumReq {
    pub album_name: Option<String>,
    pub description: Option<String>,
    pub album_thumbnail_asset_id: Option<Uuid>,
    pub is_activity_enabled: Option<bool>,
    pub order: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkIdsReq {
    pub ids: Vec<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumsAddAssetsReq {
    pub album_ids: Vec<Uuid>,
    pub asset_ids: Vec<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumUserAddReq {
    pub user_id: Uuid,
    #[serde(default = "default_editor_role")]
    pub role: String,
}

fn default_editor_role() -> String {
    "editor".to_string()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddUsersReq {
    pub album_users: Vec<AlbumUserAddReq>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAlbumUserReq {
    pub role: String,
}

impl AlbumService {
    pub fn new(pool: sqlx::PgPool, jobs: JobService) -> Self {
        Self {
            db: DbService::new(pool),
            jobs,
        }
    }

    pub async fn get_statistics(&self, auth: &AuthDto) -> Result<AlbumStatisticsResponse, ErrorResp> {
        require_permission(auth, Permission::AlbumStatistics)?;

        let owned = album::count_owned_albums(&self.db.pool, &auth.user.id).await?;
        let shared = album::count_shared_albums(&self.db.pool, &auth.user.id).await?;
        let not_shared = album::count_owned_not_shared_albums(&self.db.pool, &auth.user.id).await?;

        Ok(AlbumStatisticsResponse {
            owned,
            shared,
            not_shared,
        })
    }

    pub async fn get_all(
        &self,
        auth: &AuthDto,
        query: &GetAlbumsQuery,
    ) -> Result<Vec<AlbumResponse>, ErrorResp> {
        require_permission(auth, Permission::AlbumRead)?;
        album::update_all_album_thumbnails(&self.db.pool)
            .await
            .map_err(ErrorResp::from)?;

        let album_ids = if let Some(asset_id) = query.asset_id {
            album::list_album_ids_by_asset(&self.db.pool, &auth.user.id, &asset_id).await?
        } else {
            album::list_accessible_album_ids(
                &self.db.pool,
                &auth.user.id,
                query.is_owned,
                query.is_shared,
            )
            .await?
        };

        let mut albums = Vec::with_capacity(album_ids.len());
        for album_id in album_ids {
            if query.asset_id.is_none() {
                if let Some(id) = query.id {
                    if album_id != id {
                        continue;
                    }
                }
            }
            albums.push(self.build_response(&auth.user.id, &album_id).await?);
        }

        if query.asset_id.is_none() {
            if let Some(name) = &query.name {
                albums.retain(|album| album.album_name == *name);
            }
        }

        if !albums.is_empty() {
            let ids: Vec<Uuid> = albums.iter().map(|album| album.id).collect();
            let metadata_rows = album::get_metadata_for_ids(&self.db.pool, &ids).await?;
            let metadata_map: std::collections::HashMap<Uuid, _> = metadata_rows
                .into_iter()
                .map(|row| (row.album_id, row))
                .collect();

            for album in &mut albums {
                if let Some(metadata) = metadata_map.get(&album.id) {
                    apply_album_metadata(album, metadata);
                }
            }
        }

        Ok(albums)
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<AlbumResponse, ErrorResp> {
        require_permission(auth, Permission::AlbumRead)?;
        album::update_all_album_thumbnails(&self.db.pool)
            .await
            .map_err(ErrorResp::from)?;
        self.get_accessible(auth, id).await
    }

    pub async fn map_for_viewer(
        &self,
        viewer_id: &Uuid,
        album_id: &Uuid,
    ) -> Result<AlbumResponse, ErrorResp> {
        self.build_response(viewer_id, album_id).await
    }

    pub async fn create(&self, auth: &AuthDto, dto: &CreateAlbumReq) -> Result<AlbumResponse, ErrorResp> {
        require_permission(auth, Permission::AlbumCreate)?;

        let album_users: Vec<_> = dto
            .album_users
            .iter()
            .filter(|user| user.user_id != auth.user.id)
            .collect();

        for album_user in &album_users {
            if !album::user_exists(&self.db.pool, &album_user.user_id).await? {
                return Err(ErrorResp::BadRequest("Invalid user".to_string()));
            }
        }

        let elevated = auth
            .session
            .as_ref()
            .is_some_and(|session| session.has_elevated_permission);
        let allowed_assets: Vec<Uuid> = if dto.asset_ids.is_empty() {
            vec![]
        } else {
            assets::filter_accessible_ids(
                &self.db.pool,
                &auth.user.id,
                &dto.asset_ids,
                elevated,
                false,
            )
            .await?
        };

        let default_order = self.default_album_order(&auth.user.id).await?;
        let thumbnail_id = allowed_assets.first().copied();

        let mut tx = self.db.pool.begin().await?;

        let album_id: Uuid = sqlx::query_scalar(
            r#"
                INSERT INTO album ("albumName", description, "albumThumbnailAssetId", "order")
                VALUES ($1, COALESCE($2, ''), $3, $4)
                RETURNING id
            "#,
        )
        .bind(&dto.album_name)
        .bind(&dto.description)
        .bind(thumbnail_id)
        .bind(&default_order)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"INSERT INTO album_user ("albumId", "userId", role) VALUES ($1, $2, 'owner')"#,
        )
        .bind(album_id)
        .bind(auth.user.id)
        .execute(&mut *tx)
        .await?;

        for asset_id in &allowed_assets {
            sqlx::query(
                r#"INSERT INTO album_asset ("albumId", "assetId") VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
            )
            .bind(album_id)
            .bind(asset_id)
            .execute(&mut *tx)
            .await?;
        }

        for album_user in album_users {
            if album_user.role == "owner" {
                return Err(ErrorResp::BadRequest("Cannot add another owner".to_string()));
            }
            let role = album::parse_album_user_role(&album_user.role).ok_or_else(|| {
                ErrorResp::BadRequest(format!("Invalid album user role: {}", album_user.role))
            })?;
            if role == AlbumUserRole::Owner {
                return Err(ErrorResp::BadRequest("Cannot add another owner".to_string()));
            }
            sqlx::query(
                r#"INSERT INTO album_user ("albumId", "userId", role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"#,
            )
            .bind(album_id)
            .bind(album_user.user_id)
            .bind(role.as_str())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        for album_user in dto
            .album_users
            .iter()
            .filter(|user| user.user_id != auth.user.id)
        {
            let _ = self
                .jobs
                .queue_notify_album_invite(&album_id, &album_user.user_id, &auth.user.name)
                .await;
        }

        self.get(auth, &album_id).await
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &UpdateAlbumReq,
    ) -> Result<AlbumResponse, ErrorResp> {
        require_album_access(&self.db.pool, auth, id, Permission::AlbumUpdate).await?;

        if let Some(thumbnail_id) = dto.album_thumbnail_asset_id {
            let in_album = album::filter_asset_ids_in_album(&self.db.pool, id, &[thumbnail_id]).await?;
            if !in_album.contains(&thumbnail_id) {
                return Err(ErrorResp::BadRequest("Invalid album thumbnail".to_string()));
            }
        }

        let current = self.get_accessible(auth, id).await?;

        sqlx::query(
            r#"
                UPDATE album
                SET "albumName" = $1,
                    description = $2,
                    "albumThumbnailAssetId" = $3,
                    "isActivityEnabled" = $4,
                    "order" = $5
                WHERE id = $6
            "#,
        )
        .bind(dto.album_name.as_ref().unwrap_or(&current.album_name))
        .bind(dto.description.as_ref().unwrap_or(&current.description))
        .bind(
            dto.album_thumbnail_asset_id
                .or(current.album_thumbnail_asset_id),
        )
        .bind(
            dto.is_activity_enabled
                .unwrap_or(current.is_activity_enabled),
        )
        .bind(dto.order.as_ref().unwrap_or(&current.order))
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        self.get(auth, id).await
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_album_access(&self.db.pool, auth, id, Permission::AlbumDelete).await?;
        album::delete_album(&self.db.pool, id).await?;
        Ok(())
    }

    pub async fn add_assets(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &BulkIdsReq,
    ) -> Result<Vec<BulkIdResponse>, ErrorResp> {
        require_album_access(&self.db.pool, auth, id, Permission::AlbumAddAsset).await?;

        let existing =
            album::filter_asset_ids_in_album(&self.db.pool, id, &dto.ids).await?;
        let not_present: Vec<Uuid> = dto
            .ids
            .iter()
            .filter(|asset_id| !existing.contains(asset_id))
            .copied()
            .collect();

        let allowed: HashSet<Uuid> = if not_present.is_empty() {
            HashSet::new()
        } else {
            let elevated = auth
                .session
                .as_ref()
                .is_some_and(|session| session.has_elevated_permission);
            assets::filter_accessible_ids(
                &self.db.pool,
                &auth.user.id,
                &not_present,
                elevated,
                false,
            )
            .await?
            .into_iter()
            .collect()
        };

        let mut results = Vec::with_capacity(dto.ids.len());
        let mut new_asset_ids = Vec::new();

        for asset_id in &dto.ids {
            if existing.contains(asset_id) {
                results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::Duplicate),
                });
                continue;
            }

            if !allowed.contains(asset_id) {
                results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NoPermission),
                });
                continue;
            }

            new_asset_ids.push(*asset_id);
            results.push(BulkIdResponse {
                id: *asset_id,
                success: true,
                error: None,
            });
        }

        if !new_asset_ids.is_empty() {
            album::add_asset_ids(&self.db.pool, id, &new_asset_ids).await?;
            album::touch_album_updated_at(&self.db.pool, id)
                .await
                .map_err(ErrorResp::from)?;

            let thumbnail = album::get_album_thumbnail_asset_id(&self.db.pool, id).await?;
            if thumbnail.is_none() {
                if let Some(first_id) = new_asset_ids.first() {
                    album::set_album_thumbnail(&self.db.pool, id, first_id).await?;
                }
            }

            self.queue_album_update_notifications(id, &auth.user.id).await?;
        }

        Ok(results)
    }

    pub async fn add_assets_to_albums(
        &self,
        auth: &AuthDto,
        dto: &AlbumsAddAssetsReq,
    ) -> Result<AlbumsAddAssetsResponse, ErrorResp> {
        require_permission(auth, Permission::AlbumAddAsset)?;

        let allowed_albums =
            check_album_ids_access(&self.db.pool, auth, &dto.album_ids, Permission::AlbumAddAsset)
                .await?;
        if allowed_albums.is_empty() {
            return Ok(AlbumsAddAssetsResponse {
                success: false,
                error: Some(BulkIdErrorReason::NoPermission),
            });
        }

        let elevated = auth
            .session
            .as_ref()
            .is_some_and(|session| session.has_elevated_permission);
        let allowed_assets: HashSet<Uuid> = assets::filter_accessible_ids(
            &self.db.pool,
            &auth.user.id,
            &dto.asset_ids,
            elevated,
            false,
        )
        .await?
        .into_iter()
        .collect();

        if allowed_assets.is_empty() {
            return Ok(AlbumsAddAssetsResponse {
                success: false,
                error: Some(BulkIdErrorReason::NoPermission),
            });
        }

        let mut success = false;
        for album_id in allowed_albums {
            let existing = album::filter_asset_ids_in_album(
                &self.db.pool,
                &album_id,
                &dto.asset_ids,
            )
            .await?;
            let not_present: Vec<Uuid> = allowed_assets
                .iter()
                .filter(|asset_id| !existing.contains(asset_id))
                .copied()
                .collect();

            if not_present.is_empty() {
                continue;
            }

            album::add_asset_ids(&self.db.pool, &album_id, &not_present).await?;
            album::touch_album_updated_at(&self.db.pool, &album_id)
                .await
                .map_err(ErrorResp::from)?;
            success = true;

            let thumbnail =
                album::get_album_thumbnail_asset_id(&self.db.pool, &album_id).await?;
            if thumbnail.is_none() {
                if let Some(first_id) = not_present.first() {
                    album::set_album_thumbnail(&self.db.pool, &album_id, first_id).await?;
                }
            }

            self.queue_album_update_notifications(&album_id, &auth.user.id)
                .await?;
        }

        Ok(AlbumsAddAssetsResponse {
            success,
            error: if success {
                None
            } else {
                Some(BulkIdErrorReason::Duplicate)
            },
        })
    }

    pub async fn remove_assets(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &BulkIdsReq,
    ) -> Result<Vec<BulkIdResponse>, ErrorResp> {
        require_album_access(&self.db.pool, auth, id, Permission::AlbumRemoveAsset).await?;

        let existing =
            album::filter_asset_ids_in_album(&self.db.pool, id, &dto.ids).await?;
        let can_always_remove = album::has_album_access(
            &self.db.pool,
            &auth.user.id,
            id,
            AlbumAccessLevel::Owner,
        )
        .await?;

        let allowed: HashSet<Uuid> = if can_always_remove {
            existing.clone()
        } else {
            let asset_list: Vec<Uuid> = existing.iter().copied().collect();
            if asset_list.is_empty() {
                HashSet::new()
            } else {
                assets::filter_accessible_ids(
                    &self.db.pool,
                    &auth.user.id,
                    &asset_list,
                    auth.session
                        .as_ref()
                        .is_some_and(|session| session.has_elevated_permission),
                    false,
                )
                .await?
                .into_iter()
                .collect()
            }
        };

        let mut results = Vec::with_capacity(dto.ids.len());
        let mut removed_ids = Vec::new();

        for asset_id in &dto.ids {
            if !existing.contains(asset_id) {
                results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NotFound),
                });
                continue;
            }

            if !allowed.contains(asset_id) {
                results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NoPermission),
                });
                continue;
            }

            removed_ids.push(*asset_id);
            results.push(BulkIdResponse {
                id: *asset_id,
                success: true,
                error: None,
            });
        }

        if !removed_ids.is_empty() {
            let thumbnail = album::get_album_thumbnail_asset_id(&self.db.pool, id).await?;
            album::remove_asset_ids(&self.db.pool, id, &removed_ids).await?;

            if thumbnail.is_some_and(|thumb| removed_ids.contains(&thumb)) {
                album::update_album_thumbnails(&self.db.pool, id).await?;
            }
        }

        Ok(results)
    }

    pub async fn add_users(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &AddUsersReq,
    ) -> Result<AlbumResponse, ErrorResp> {
        require_album_access(&self.db.pool, auth, id, Permission::AlbumShare).await?;

        for album_user in &dto.album_users {
            if album_user.user_id == auth.user.id {
                continue;
            }

            if album_user.role == "owner" {
                return Err(ErrorResp::BadRequest("Cannot add another owner".to_string()));
            }

            let role = album::parse_album_user_role(&album_user.role).ok_or_else(|| {
                ErrorResp::BadRequest(format!("Invalid album user role: {}", album_user.role))
            })?;
            if role == AlbumUserRole::Owner {
                return Err(ErrorResp::BadRequest("Cannot add another owner".to_string()));
            }

            if album::album_user_exists(&self.db.pool, id, &album_user.user_id)
                .await?
                .is_some()
            {
                continue;
            }

            if !album::user_exists(&self.db.pool, &album_user.user_id).await? {
                return Err(ErrorResp::BadRequest("Invalid user".to_string()));
            }

            album::add_album_user(&self.db.pool, id, &album_user.user_id, role).await?;
            let _ = self
                .jobs
                .queue_notify_album_invite(id, &album_user.user_id, &auth.user.name)
                .await;
        }

        self.get(auth, id).await
    }

    pub async fn update_user(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        user_id: &Uuid,
        dto: &UpdateAlbumUserReq,
    ) -> Result<(), ErrorResp> {
        require_album_access(&self.db.pool, auth, id, Permission::AlbumShare).await?;

        let role = album::parse_album_user_role(&dto.role).ok_or_else(|| {
            ErrorResp::BadRequest(format!("Invalid album user role: {}", dto.role))
        })?;

        if album::album_user_exists(&self.db.pool, id, user_id)
            .await?
            .is_none()
        {
            return Err(ErrorResp::BadRequest("Album not shared with user".to_string()));
        }

        album::update_album_user_role(&self.db.pool, id, user_id, role).await?;
        Ok(())
    }

    pub async fn remove_user(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        user_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        if auth.user.id != *user_id {
            require_album_access(&self.db.pool, auth, id, Permission::AlbumShare).await?;
        }

        let role = album::album_user_exists(&self.db.pool, id, user_id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Album not shared with user".to_string()))?;

        if role == "owner" {
            let owner_count = album::count_album_owners(&self.db.pool, id).await?;
            if owner_count <= 1 {
                return Err(ErrorResp::BadRequest(
                    "Cannot remove the last album owner".to_string(),
                ));
            }
        }

        album::remove_album_user(&self.db.pool, id, user_id).await?;
        Ok(())
    }

    async fn get_accessible(&self, auth: &AuthDto, id: &Uuid) -> Result<AlbumResponse, ErrorResp> {
        require_album_access(&self.db.pool, auth, id, Permission::AlbumRead).await?;
        self.build_response(&auth.user.id, id).await
    }

    async fn build_response(
        &self,
        auth_user_id: &Uuid,
        album_id: &Uuid,
    ) -> Result<AlbumResponse, ErrorResp> {
        let row = album::get_album_row(&self.db.pool, album_id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Album not found".to_string()))?;

        let users = album::get_album_users(&self.db.pool, album_id, auth_user_id).await?;
        let has_shared_link = album::album_has_shared_link(&self.db.pool, album_id).await?;
        let asset_count = album::count_album_assets(&self.db.pool, album_id).await?;
        let has_shared_users = users.len() > 1;
        let is_shared = has_shared_users || has_shared_link;

        let metadata_rows = album::get_metadata_for_ids(&self.db.pool, &[*album_id]).await?;
        let metadata = metadata_rows.into_iter().next();

        let contributor_counts = if is_shared {
            Some(
                album::get_contributor_counts(&self.db.pool, album_id)
                    .await?
                    .into_iter()
                    .map(|row| ContributorCountResponse {
                        user_id: row.user_id,
                        asset_count: row.asset_count,
                    })
                    .collect(),
            )
        } else {
            None
        };

        let mut response = AlbumResponse {
            id: row.id,
            album_name: row.album_name,
            description: row.description,
            created_at: format_album_datetime(&row.created_at),
            updated_at: format_album_datetime(&row.updated_at),
            album_thumbnail_asset_id: row.album_thumbnail_asset_id,
            shared: is_shared,
            album_users: users
                .into_iter()
                .map(|user| {
                    let avatar_color = user
                        .avatar_color
                        .filter(|color| !color.is_empty())
                        .unwrap_or_else(|| email_to_avatar_color(&user.email));
                    AlbumUserResponse {
                        user: AlbumUserInfo {
                            id: user.user_id,
                            name: user.name,
                            email: user.email,
                            profile_image_path: user.profile_image_path,
                            avatar_color,
                            profile_changed_at: format_album_datetime(&user.profile_changed_at),
                        },
                        role: user.role,
                    }
                })
                .collect(),
            has_shared_link,
            asset_count,
            last_modified_asset_timestamp: None,
            start_date: None,
            end_date: None,
            is_activity_enabled: row.is_activity_enabled,
            order: row.order,
            contributor_counts,
        };

        if let Some(metadata) = metadata {
            apply_album_metadata(&mut response, &metadata);
        }

        Ok(response)
    }

    async fn queue_album_update_notifications(
        &self,
        album_id: &Uuid,
        actor_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        let members = album::list_album_member_ids(&self.db.pool, album_id).await?;
        for recipient_id in members {
            if recipient_id == *actor_id {
                continue;
            }
            self.jobs
                .queue_notify_album_update(album_id, &recipient_id)
                .await?;
        }
        Ok(())
    }

    async fn default_album_order(&self, user_id: &Uuid) -> Result<String, ErrorResp> {
        let stored = UserMetadataPO::get_preferences_json(&self.db.pool, user_id)
            .await
            .map_err(ErrorResp::from)?;
        let prefs = resolve_preferences(stored);
        Ok(prefs
            .get("albums")
            .and_then(|value| value.get("defaultAssetOrder"))
            .and_then(|value| value.as_str())
            .unwrap_or("desc")
            .to_string())
    }
}

fn apply_album_metadata(album: &mut AlbumResponse, metadata: &album::AlbumMetadataRow) {
    album.asset_count = metadata.asset_count;
    album.start_date = metadata.start_date.as_ref().map(format_album_datetime);
    album.end_date = metadata.end_date.as_ref().map(format_album_datetime);
    album.last_modified_asset_timestamp = metadata
        .last_modified_asset_timestamp
        .as_ref()
        .map(format_album_datetime);

    if let (Some(start), Some(end)) = (&album.start_date, &album.end_date) {
        if start > end {
            std::mem::swap(&mut album.start_date, &mut album.end_date);
        }
    }
}

fn format_album_datetime(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn email_to_avatar_color(email: &str) -> String {
    const COLORS: [&str; 10] = [
        "primary", "pink", "blue", "green", "yellow", "red", "purple", "orange", "gray", "amber",
    ];
    let sum: u32 = email.chars().map(|ch| ch as u32).sum();
    COLORS[(sum as usize) % COLORS.len()].to_string()
}
