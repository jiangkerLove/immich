use sqlx::PgPool;
use uuid::Uuid;

use crate::ext::bcrypt::hash_bcrypt;
use crate::models::db::assets::{self, AssetStatsRow};
use crate::models::db::auth_permission::Permission;
use crate::models::db::user_metadata::UserMetadataPO;
use crate::models::db::users::{UserDb, map_user_admin, map_user_admin_with_license};
use crate::models::dto::auth::AuthDto;
use crate::models::request::user::UserPreferencesUpdateReq;
use crate::models::response::asset::AssetStatsResponse;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::service::asset::AssetStatsQuery;
use crate::service::job::JobService;
use crate::service::session::SessionResponse;
use crate::service::websocket::WebSocketHub;
use crate::utils::permission::{require_admin, require_permission};
use crate::utils::preferences::{merge_preferences, resolve_preferences};
use crate::utils::query::parse_query_bool;

#[derive(Clone)]
pub struct UserAdminService {
    pool: PgPool,
    websocket: WebSocketHub,
    jobs: JobService,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAdminSearchQuery {
    pub id: Option<Uuid>,
    pub with_deleted: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAdminCreateReq {
    pub email: String,
    pub password: String,
    pub name: String,
    pub avatar_color: Option<Option<String>>,
    pub pin_code: Option<Option<String>>,
    pub storage_label: Option<Option<String>>,
    pub quota_size_in_bytes: Option<Option<i64>>,
    pub should_change_password: Option<bool>,
    pub notify: Option<bool>,
    pub is_admin: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAdminUpdateReq {
    pub email: Option<String>,
    pub password: Option<String>,
    pub pin_code: Option<Option<String>>,
    pub name: Option<String>,
    pub avatar_color: Option<Option<String>>,
    pub storage_label: Option<Option<String>>,
    pub should_change_password: Option<bool>,
    pub quota_size_in_bytes: Option<Option<i64>>,
    pub is_admin: Option<bool>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAdminDeleteReq {
    pub force: Option<bool>,
}

impl UserAdminService {
    pub fn new(pool: PgPool, websocket: WebSocketHub, jobs: JobService) -> Self {
        Self {
            pool,
            websocket,
            jobs,
        }
    }

    pub async fn search(
        &self,
        auth: &AuthDto,
        query: &UserAdminSearchQuery,
    ) -> Result<Vec<UserAdminResponse>, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::AdminUserRead)?;

        let with_deleted = query
            .with_deleted
            .as_deref()
            .and_then(parse_query_bool)
            .unwrap_or(false);

        let users = UserDb::list_admin(&self.pool, query.id.as_ref(), with_deleted).await?;
        Ok(users
            .into_iter()
            .map(|user| map_user_admin(user, None))
            .collect())
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &UserAdminCreateReq,
    ) -> Result<UserAdminResponse, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::AdminUserCreate)?;

        if dto.password.is_empty() {
            return Err(ErrorResp::BadRequest("password is required".to_string()));
        }

        if let Some(existing) = UserDb::get_by_email(&self.pool, &dto.email).await? {
            if existing.deleted_at.is_none() {
                return Err(ErrorResp::BadRequest("Email is not available".to_string()));
            }
        }

        if let Some(Some(label)) = &dto.storage_label {
            if UserDb::get_by_storage_label(&self.pool, label)
                .await?
                .is_some()
            {
                return Err(ErrorResp::BadRequest(
                    "Storage label already in use by another account".to_string(),
                ));
            }
        }

        if let Some(Some(pin)) = &dto.pin_code {
            if !is_valid_pin_code(pin) {
                return Err(ErrorResp::BadRequest("Invalid PIN code".to_string()));
            }
        }

        let password_hash =
            hash_bcrypt(&dto.password).map_err(|err| ErrorResp::ServerError(err.to_string()))?;

        let pin_hash = match &dto.pin_code {
            Some(Some(pin)) => {
                Some(hash_bcrypt(pin).map_err(|err| ErrorResp::ServerError(err.to_string()))?)
            }
            Some(None) => None,
            None => None,
        };

        let user = UserDb::admin_create(
            &self.pool,
            &dto.email,
            &password_hash,
            &dto.name,
            dto.is_admin.unwrap_or(false),
            dto.storage_label.as_ref().and_then(|v| v.as_deref()),
            dto.avatar_color.as_ref().and_then(|v| v.as_deref()),
            pin_hash.as_deref(),
            dto.quota_size_in_bytes.as_ref().and_then(|v| *v),
            dto.should_change_password.unwrap_or(false),
        )
        .await?;

        if dto.notify.unwrap_or(false) {
            let _ = self
                .jobs
                .queue_notify_user_signup(&user.id, Some(dto.password.clone()))
                .await;
        }

        crate::utils::telemetry::add_users_total(1);
        Ok(map_user_admin_with_license(&self.pool, user).await?)
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<UserAdminResponse, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::AdminUserRead)?;

        let user = self.find_or_fail(id, true).await?;
        Ok(map_user_admin_with_license(&self.pool, user).await?)
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &UserAdminUpdateReq,
    ) -> Result<UserAdminResponse, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::AdminUserUpdate)?;

        let user = self.find_or_fail(id, false).await?;

        if let Some(is_admin) = dto.is_admin {
            if is_admin != auth.user.is_admin && auth.user.id == *id {
                return Err(ErrorResp::BadRequest(
                    "Admin status can only be changed by another admin".to_string(),
                ));
            }
        }

        if let Some(quota) = dto.quota_size_in_bytes {
            if quota != user.quota_size_in_bytes {
                UserDb::sync_usage(&self.pool, id).await?;
            }
        }

        if let Some(email) = &dto.email {
            if let Some(existing) = UserDb::get_by_email(&self.pool, email).await? {
                if existing.id != *id {
                    return Err(ErrorResp::BadRequest("Email is not available".to_string()));
                }
            }
        }

        if let Some(Some(label)) = &dto.storage_label {
            if let Some(existing) = UserDb::get_by_storage_label(&self.pool, label).await? {
                if existing.id != *id {
                    return Err(ErrorResp::BadRequest(
                        "Storage label already in use by another account".to_string(),
                    ));
                }
            }
        }

        let password_hash = if let Some(password) = &dto.password {
            Some(hash_bcrypt(password).map_err(|err| ErrorResp::ServerError(err.to_string()))?)
        } else {
            None
        };

        let pin_code = match &dto.pin_code {
            Some(Some(pin)) => {
                if !is_valid_pin_code(pin) {
                    return Err(ErrorResp::BadRequest("Invalid PIN code".to_string()));
                }
                Some(Some(
                    hash_bcrypt(pin).map_err(|err| ErrorResp::ServerError(err.to_string()))?,
                ))
            }
            Some(None) => Some(None),
            None => None,
        };

        let storage_label = dto
            .storage_label
            .as_ref()
            .map(|value| value.as_ref().map(|s| s.as_str()).filter(|s| !s.is_empty()));

        let user = UserDb::admin_update(
            &self.pool,
            id,
            dto.email.as_deref(),
            password_hash.as_deref(),
            dto.name.as_deref(),
            dto.avatar_color
                .as_ref()
                .map(|value| value.as_ref().map(|s| s.as_str())),
            pin_code.as_ref().map(|value| value.as_deref()),
            storage_label,
            dto.quota_size_in_bytes,
            dto.should_change_password,
            dto.is_admin,
        )
        .await?;

        Ok(map_user_admin_with_license(&self.pool, user).await?)
    }

    pub async fn delete(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &UserAdminDeleteReq,
    ) -> Result<UserAdminResponse, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::AdminUserDelete)?;

        self.find_or_fail(id, false).await?;

        if auth.user.id == *id {
            return Err(ErrorResp::Forbidden(
                "Cannot delete your own account".to_string(),
            ));
        }

        let force = dto.force.unwrap_or(false);
        let user = UserDb::admin_delete(&self.pool, id, force).await?;
        // Match TS: soft-delete emits UserTrash (telemetry only); on_user_delete comes from UserDelete job.
        crate::utils::telemetry::add_users_total(-1);
        if force {
            self.jobs.queue_user_delete(id, true).await?;
        }
        Ok(map_user_admin_with_license(&self.pool, user).await?)
    }

    pub async fn restore(&self, auth: &AuthDto, id: &Uuid) -> Result<UserAdminResponse, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::AdminUserDelete)?;

        self.find_or_fail(id, true).await?;
        let user = UserDb::admin_restore(&self.pool, id).await?;
        crate::utils::telemetry::add_users_total(1);
        Ok(map_user_admin_with_license(&self.pool, user).await?)
    }

    pub async fn get_calendar_heatmap(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        query: &crate::utils::calendar_heatmap::CalendarHeatmapQuery,
    ) -> Result<crate::utils::calendar_heatmap::CalendarHeatmapResponse, ErrorResp> {
        require_permission(auth, Permission::UserRead)?;
        self.find_or_fail(id, false).await?;
        crate::utils::calendar_heatmap::build_calendar_heatmap(&self.pool, id, query).await
    }

    pub async fn get_sessions(
        &self,
        auth: &AuthDto,
        id: &Uuid,
    ) -> Result<Vec<SessionResponse>, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::AdminSessionRead)?;
        self.find_or_fail(id, false).await?;

        crate::service::session::list_sessions_for_user(&self.pool, id).await
    }

    pub async fn get_statistics(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        query: &AssetStatsQuery,
    ) -> Result<AssetStatsResponse, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::AdminUserRead)?;

        if query.visibility.as_deref() == Some("locked") {
            let elevated = auth
                .session
                .as_ref()
                .is_some_and(|s| s.has_elevated_permission);
            if !elevated {
                return Err(ErrorResp::Forbidden("Forbidden".to_string()));
            }
        }

        self.find_or_fail(id, false).await?;

        let stats = assets::get_statistics(
            &self.pool,
            id,
            query.visibility.as_deref(),
            query.is_favorite.as_deref().and_then(parse_query_bool),
            query
                .is_trashed
                .as_deref()
                .and_then(parse_query_bool)
                .unwrap_or(false),
        )
        .await?;

        Ok(map_stats(&stats))
    }

    pub async fn get_preferences(
        &self,
        auth: &AuthDto,
        id: &Uuid,
    ) -> Result<serde_json::Value, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::AdminUserRead)?;
        self.find_or_fail(id, true).await?;

        let stored = UserMetadataPO::get_preferences_json(&self.pool, id).await?;
        Ok(resolve_preferences(stored))
    }

    pub async fn update_preferences(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &UserPreferencesUpdateReq,
    ) -> Result<serde_json::Value, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::AdminUserUpdate)?;
        self.find_or_fail(id, false).await?;

        let stored = UserMetadataPO::get_preferences_json(&self.pool, id).await?;
        let mut preferences = resolve_preferences(stored);
        merge_preferences(&mut preferences, dto.clone());

        UserMetadataPO::upsert_preferences_json(&self.pool, id, &preferences).await?;
        Ok(preferences)
    }

    async fn find_or_fail(&self, id: &Uuid, with_deleted: bool) -> Result<UserDb, ErrorResp> {
        UserDb::select_by_id_admin(&self.pool, id, with_deleted)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("User not found".to_string()))
    }
}

fn map_stats(stats: &AssetStatsRow) -> AssetStatsResponse {
    AssetStatsResponse {
        images: stats.image,
        videos: stats.video,
        total: stats.image + stats.video + stats.audio + stats.other,
    }
}

fn is_valid_pin_code(pin_code: &str) -> bool {
    pin_code.len() == 6 && pin_code.chars().all(|c| c.is_ascii_digit())
}
