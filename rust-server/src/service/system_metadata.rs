use sqlx::PgPool;

use crate::models::db::system_metadata::{
    self, AdminOnboarding, ReverseGeocodingState, VersionCheckState,
};
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::models::db::auth_permission::Permission;
use crate::utils::permission::{require_admin, require_permission};

#[derive(Clone)]
pub struct SystemMetadataService {
    pool: PgPool,
}

impl SystemMetadataService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_admin_onboarding(
        &self,
        auth: &AuthDto,
    ) -> Result<AdminOnboarding, ErrorResp> {
        require_permission(auth, Permission::SystemMetadataRead)?;
        require_admin(auth)?;
        system_metadata::get_admin_onboarding(&self.pool)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn update_admin_onboarding(
        &self,
        auth: &AuthDto,
        dto: &AdminOnboarding,
    ) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::SystemMetadataUpdate)?;
        require_admin(auth)?;
        system_metadata::set_admin_onboarding(&self.pool, dto)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn get_reverse_geocoding_state(
        &self,
        auth: &AuthDto,
    ) -> Result<ReverseGeocodingState, ErrorResp> {
        require_permission(auth, Permission::SystemMetadataRead)?;
        require_admin(auth)?;
        system_metadata::get_reverse_geocoding_state(&self.pool)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn get_version_check_state(
        &self,
        auth: &AuthDto,
    ) -> Result<VersionCheckState, ErrorResp> {
        require_permission(auth, Permission::SystemMetadataRead)?;
        require_admin(auth)?;
        system_metadata::get_version_check_state(&self.pool)
            .await
            .map_err(ErrorResp::from)
    }
}
