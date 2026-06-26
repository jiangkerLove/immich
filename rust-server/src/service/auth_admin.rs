use sqlx::PgPool;

use crate::models::db::auth_permission::Permission;
use crate::models::db::users::UserDb;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::utils::permission::{require_admin, require_permission};

#[derive(Clone)]
pub struct AuthAdminService {
    pool: PgPool,
}

impl AuthAdminService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn unlink_all(&self, auth: &AuthDto) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::AdminAuthUnlinkAll)?;
        require_admin(auth)?;
        UserDb::unlink_all_oauth(&self.pool)
            .await
            .map_err(ErrorResp::from)
    }
}
