use sqlx::PgPool;

use crate::models::db::auth_permission::Permission;
use crate::models::db::view;
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::{map_assets, AssetResponse};
use crate::models::response::response::ErrorResp;
use crate::utils::permission::require_permission;

#[derive(Clone)]
pub struct ViewService {
    pool: PgPool,
}

impl ViewService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_unique_original_paths(&self, auth: &AuthDto) -> Result<Vec<String>, ErrorResp> {
        require_permission(auth, Permission::FolderRead)?;
        Ok(view::get_unique_original_paths(&self.pool, &auth.user.id).await?)
    }

    pub async fn get_assets_by_original_path(
        &self,
        auth: &AuthDto,
        path: &str,
    ) -> Result<Vec<AssetResponse>, ErrorResp> {
        require_permission(auth, Permission::FolderRead)?;
        let rows = view::get_assets_by_original_path(&self.pool, &auth.user.id, path).await?;
        Ok(map_assets(&self.pool, &rows, auth, false).await?)
    }
}
