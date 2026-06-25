use crate::models::db::user_metadata::{OnboardingPO, UserMetadataKey, UserMetadataPO, UserPreferencePO};
use crate::models::db::users::{map_user_admin, UserDb};
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::models::response::user_preferences_response::UserPreferenceResponse;
use crate::service::db::DbService;
use crate::utils::permission::require_permission;
use crate::models::db::auth_permission::Permission;

#[derive(Clone)]
pub struct UserService {
    db: DbService,
}

impl UserService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            db: DbService::new(pool),
        }
    }

    pub async fn get_me(&self, auth: &AuthDto) -> Result<UserAdminResponse, ErrorResp> {
        require_permission(auth, Permission::UserRead)?;
        let user = UserDb::select_full_by_id(&self.db.pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::ServerError("User not found".to_string()))?;
        Ok(map_user_admin(user))
    }

    pub async fn get_me_preferences(
        &self,
        auth: &AuthDto,
    ) -> Result<UserPreferenceResponse, ErrorResp> {
        require_permission(auth, Permission::UserPreferenceRead)?;
        let user_meta = UserMetadataPO::get_meta_data_by_uid(&self.db.pool, &auth.user.id).await?;
        Ok(user_meta
            .into_iter()
            .find(|item| item.key == UserMetadataKey::Preferences.as_str())
            .map(|item| UserPreferenceResponse::from(item.value.0))
            .unwrap_or_else(|| UserPreferenceResponse::from(UserPreferencePO::default())))
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

        Ok(users.into_iter().map(map_user_admin).collect())
    }

    pub async fn get_my_onboarding(
        &self,
        auth: &AuthDto,
    ) -> Result<OnboardingPO, ErrorResp> {
        require_permission(auth, Permission::UserRead)?;
        UserMetadataPO::get_onboarding(&self.db.pool, &auth.user.id)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn set_my_onboarding(
        &self,
        auth: &AuthDto,
        onboarding: &OnboardingPO,
    ) -> Result<OnboardingPO, ErrorResp> {
        require_permission(auth, Permission::UserRead)?;
        UserMetadataPO::upsert_onboarding(&self.db.pool, &auth.user.id, onboarding)
            .await
            .map_err(ErrorResp::from)?;
        Ok(onboarding.clone())
    }
}
