use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

use crate::models::db::auth_permission::Permission;
use crate::models::db::users::AuthUserDb;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthApiKey {
    pub id: String,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, FromRow)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub permissions: Vec<String>,
    pub user_id: Uuid,
    pub is_admin: bool,
    pub name: String,
    pub email: String,
    pub quota_usage_in_bytes: i64,
    pub quota_size_in_bytes: Option<i64>,
}

impl ApiKeyRow {
    pub async fn get_by_key(
        pool: &Pool<Postgres>,
        hashed_key: &[u8],
    ) -> Result<Option<ApiKeyRow>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"
                SELECT
                    api_key.id,
                    api_key.permissions,
                    api_key."userId" as "user_id",
                    "user"."isAdmin" as "is_admin",
                    "user".name,
                    "user".email,
                    "user"."quotaUsageInBytes" as "quota_usage_in_bytes",
                    "user"."quotaSizeInBytes" as "quota_size_in_bytes"
                FROM api_key
                INNER JOIN "user" ON "user".id = api_key."userId"
                WHERE api_key.key = $1
                  AND "user"."deletedAt" IS NULL
            "#,
        )
        .bind(hashed_key)
        .fetch_optional(pool)
        .await
    }

    pub fn into_auth(self) -> (AuthUserDb, AuthApiKey) {
        let user = AuthUserDb {
            id: self.user_id,
            is_admin: self.is_admin,
            name: self.name,
            email: self.email,
            quota_usage_in_bytes: self.quota_usage_in_bytes,
            quota_size_in_bytes: self.quota_size_in_bytes,
        };
        let permissions = self
            .permissions
            .iter()
            .filter_map(|permission| Permission::from_str(permission))
            .collect();
        let api_key = AuthApiKey {
            id: self.id.to_string(),
            permissions,
        };
        (user, api_key)
    }
}
