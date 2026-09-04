use uuid::Uuid;

use crate::ext::bcrypt::hash_bcrypt;
use crate::models::db::auth_permission::Permission;
use crate::models::db::user_metadata::UserMetadataPO;
use crate::models::db::users::{UserDb, map_user, map_user_admin, map_user_admin_with_license};
use crate::models::dto::auth::AuthDto;
use crate::models::request::user::{UpdateUserMeReq, UserPreferencesUpdateReq};
use crate::models::response::response::ErrorResp;
use crate::models::response::user::{UserAdminResponse, UserResponse};
use crate::service::db::DbService;
use crate::service::job::JobService;
use crate::utils::file_response::{FileResponse, file_response, guess_mime};
use crate::utils::permission::require_permission;
use crate::utils::preferences::{merge_preferences, resolve_preferences};
use crate::utils::profile_image;
use crate::utils::storage::StoragePaths;

#[derive(Clone)]
pub struct UserService {
    db: DbService,
    storage: StoragePaths,
    jobs: JobService,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProfileImageResponse {
    pub user_id: uuid::Uuid,
    pub profile_changed_at: chrono::DateTime<chrono::Utc>,
    pub profile_image_path: String,
}

impl UserService {
    pub fn new(pool: sqlx::PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self {
            db: DbService::new(pool),
            storage,
            jobs,
        }
    }

    pub async fn get_me(&self, auth: &AuthDto) -> Result<UserAdminResponse, ErrorResp> {
        require_permission(auth, Permission::UserRead)?;
        let user = UserDb::select_full_by_id(&self.db.pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::ServerError("User not found".to_string()))?;
        Ok(map_user_admin_with_license(&self.db.pool, user).await?)
    }

    pub async fn get_me_preferences(&self, auth: &AuthDto) -> Result<serde_json::Value, ErrorResp> {
        require_permission(auth, Permission::UserPreferenceRead)?;
        let stored = UserMetadataPO::get_preferences_json(&self.db.pool, &auth.user.id).await?;
        Ok(resolve_preferences(stored))
    }

    pub async fn update_me(
        &self,
        auth: &AuthDto,
        dto: &UpdateUserMeReq,
    ) -> Result<UserAdminResponse, ErrorResp> {
        require_permission(auth, Permission::UserUpdate)?;

        if let Some(email) = &dto.email {
            if let Some(existing) = UserDb::get_by_email(&self.db.pool, email).await? {
                if existing.id != auth.user.id {
                    return Err(ErrorResp::BadRequest("Email is not available".to_string()));
                }
            }
        }

        let password_hash = if let Some(password) = &dto.password {
            Some(hash_bcrypt(password).map_err(|err| ErrorResp::ServerError(err.to_string()))?)
        } else {
            None
        };

        let avatar_color = dto
            .avatar_color
            .as_ref()
            .map(|value| value.as_ref().map(|color| color.as_str()));

        let user = UserDb::update_me(
            &self.db.pool,
            &auth.user.id,
            dto.email.as_deref(),
            dto.name.as_deref(),
            avatar_color,
            password_hash.as_deref(),
        )
        .await?;

        Ok(map_user_admin_with_license(&self.db.pool, user).await?)
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<UserResponse, ErrorResp> {
        require_permission(auth, Permission::UserRead)?;
        let user = UserDb::select_full_by_id(&self.db.pool, id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("User not found".to_string()))?;
        Ok(map_user(user))
    }

    pub async fn update_my_preferences(
        &self,
        auth: &AuthDto,
        dto: &UserPreferencesUpdateReq,
    ) -> Result<serde_json::Value, ErrorResp> {
        require_permission(auth, Permission::UserPreferenceUpdate)?;

        let stored = UserMetadataPO::get_preferences_json(&self.db.pool, &auth.user.id).await?;
        let mut preferences = resolve_preferences(stored);
        merge_preferences(&mut preferences, dto.clone());

        UserMetadataPO::upsert_preferences_json(&self.db.pool, &auth.user.id, &preferences).await?;
        Ok(preferences)
    }

    pub async fn get_calendar_heatmap(
        &self,
        auth: &AuthDto,
        query: &crate::utils::calendar_heatmap::CalendarHeatmapQuery,
    ) -> Result<crate::utils::calendar_heatmap::CalendarHeatmapResponse, ErrorResp> {
        require_permission(auth, Permission::UserRead)?;
        crate::utils::calendar_heatmap::build_calendar_heatmap(&self.db.pool, &auth.user.id, query)
            .await
    }

    pub async fn search(&self, auth: &AuthDto) -> Result<Vec<UserAdminResponse>, ErrorResp> {
        require_permission(auth, Permission::UserRead)?;
        let users = sqlx::query_as::<_, UserDb>(
            r#"
                SELECT
                    id, "createdAt" as created_at, "profileImagePath" as profile_image_path,
                    "shouldChangePassword" as should_change_password, "deletedAt" as deleted_at,
                    "oauthId" as oauth_id, "updatedAt" as updated_at, "storageLabel" as storage_label,
                    name, "quotaSizeInBytes" as quota_size_in_bytes,
                    "quotaUsageInBytes" as quota_usage_in_bytes, status,
                    "profileChangedAt" as profile_changed_at, "updateId" as update_id,
                    "avatarColor" as avatar_color, "pinCode" as pin_code,
                    email, password, "isAdmin" as is_admin
                FROM "user"
                WHERE "deletedAt" IS NULL
                ORDER BY name
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        Ok(users
            .into_iter()
            .map(|user| map_user_admin(user, None))
            .collect())
    }

    pub async fn get_my_onboarding(
        &self,
        auth: &AuthDto,
    ) -> Result<crate::models::db::user_metadata::OnboardingPO, ErrorResp> {
        require_permission(auth, Permission::UserRead)?;
        UserMetadataPO::get_onboarding(&self.db.pool, &auth.user.id)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn set_my_onboarding(
        &self,
        auth: &AuthDto,
        onboarding: &crate::models::db::user_metadata::OnboardingPO,
    ) -> Result<crate::models::db::user_metadata::OnboardingPO, ErrorResp> {
        require_permission(auth, Permission::UserRead)?;
        UserMetadataPO::upsert_onboarding(&self.db.pool, &auth.user.id, onboarding)
            .await
            .map_err(ErrorResp::from)?;
        Ok(onboarding.clone())
    }

    pub async fn delete_my_onboarding(&self, auth: &AuthDto) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::UserOnboardingDelete)?;
        UserMetadataPO::delete_onboarding(&self.db.pool, &auth.user.id)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn get_my_license(
        &self,
        auth: &AuthDto,
    ) -> Result<crate::models::response::user::UserLicenseResponse, ErrorResp> {
        require_permission(auth, Permission::UserLicenseRead)?;
        let license = UserMetadataPO::get_license(&self.db.pool, &auth.user.id)
            .await
            .map_err(ErrorResp::from)?
            .ok_or_else(|| ErrorResp::BadRequest("License not found".to_string()))?;
        Ok(crate::models::db::users::map_license(license))
    }

    pub async fn set_my_license(
        &self,
        auth: &AuthDto,
        dto: &crate::service::server::LicenseKeyReq,
    ) -> Result<crate::models::response::user::UserLicenseResponse, ErrorResp> {
        use crate::utils::license::{is_valid_user_license_prefix, verify_user_license};

        require_permission(auth, Permission::UserLicenseUpdate)?;

        if !is_valid_user_license_prefix(&dto.license_key)
            || !verify_user_license(&dto.license_key, &dto.activation_key)
        {
            return Err(ErrorResp::BadRequest("Invalid license key".to_string()));
        }

        let activated_at = chrono::Utc::now().to_rfc3339();
        let license = crate::models::db::user_metadata::UserLicensePO {
            license_key: dto.license_key.clone(),
            activation_key: dto.activation_key.clone(),
            activated_at: activated_at.clone(),
        };
        UserMetadataPO::upsert_license(&self.db.pool, &auth.user.id, &license)
            .await
            .map_err(ErrorResp::from)?;
        Ok(crate::models::db::users::map_license(license))
    }

    pub async fn delete_my_license(&self, auth: &AuthDto) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::UserLicenseDelete)?;
        UserMetadataPO::delete_license(&self.db.pool, &auth.user.id)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn create_profile_image(
        &self,
        auth: &AuthDto,
        file_bytes: Vec<u8>,
        _original_name: &str,
    ) -> Result<CreateProfileImageResponse, ErrorResp> {
        require_permission(auth, Permission::UserProfileImageUpdate)?;

        let current = UserDb::select_full_by_id(&self.db.pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("User not found".to_string()))?;
        let old_path = current.profile_image_path.clone();

        let profile_path = profile_image::generate_profile_image(
            &self.db.pool,
            &self.storage,
            &auth.user.id,
            &file_bytes,
        )
        .await
        .map_err(|_| ErrorResp::BadRequest("Unable to process profile image".to_string()))?;

        let profile_path_str = profile_path.to_string_lossy().to_string();
        let user =
            UserDb::update_profile_image(&self.db.pool, &auth.user.id, &profile_path_str).await?;

        if !old_path.is_empty() {
            self.jobs.queue_file_delete(&[old_path]).await?;
        }

        Ok(CreateProfileImageResponse {
            user_id: user.id,
            profile_changed_at: user.profile_changed_at,
            profile_image_path: user.profile_image_path,
        })
    }

    pub async fn delete_profile_image(&self, auth: &AuthDto) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::UserProfileImageDelete)?;

        let current = UserDb::select_full_by_id(&self.db.pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("User not found".to_string()))?;
        if current.profile_image_path.is_empty() {
            return Err(ErrorResp::BadRequest(
                "Can't delete a missing profile Image".to_string(),
            ));
        }

        let old_path = current.profile_image_path.clone();
        UserDb::clear_profile_image(&self.db.pool, &auth.user.id).await?;
        self.jobs.queue_file_delete(&[old_path]).await?;
        Ok(())
    }

    pub async fn get_profile_image(
        &self,
        auth: &AuthDto,
        user_id: &uuid::Uuid,
    ) -> Result<axum::response::Response, ErrorResp> {
        require_permission(auth, Permission::UserProfileImageRead)?;
        let path = UserDb::get_profile_image_path(&self.db.pool, user_id)
            .await?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ErrorResp::BadRequest("User not found".to_string()))?;
        let content_type = guess_mime(&path);

        file_response(FileResponse {
            path,
            content_type,
            file_name: None,
            cache_control: Some("no-store".to_string()),
        })
        .await
    }
}
