use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::db::DbService;
use crate::utils::crypto::{hash_sha256, random_bytes_as_text};
use crate::utils::permission::require_permission;

#[derive(Clone)]
pub struct ApiKeyService {
    db: DbService,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyResponse {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub permissions: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyCreateReq {
    pub name: String,
    pub permissions: Vec<String>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyUpdateReq {
    pub name: Option<String>,
    pub permissions: Option<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyCreateResp {
    #[serde(flatten)]
    pub api_key: ApiKeyResponse,
    pub secret: String,
}

impl ApiKeyService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            db: DbService::new(pool),
        }
    }

    pub async fn get_all(&self, auth: &AuthDto) -> Result<Vec<ApiKeyResponse>, ErrorResp> {
        require_permission(auth, Permission::ApiKeyRead)?;
        sqlx::query_as::<_, ApiKeyResponse>(
            r#"
                SELECT id, name, "createdAt" as created_at, "updatedAt" as updated_at, permissions
                FROM api_key
                WHERE "userId" = $1
                ORDER BY "createdAt" DESC
            "#,
        )
        .bind(auth.user.id)
        .fetch_all(&self.db.pool)
        .await
        .map_err(ErrorResp::from)
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<ApiKeyResponse, ErrorResp> {
        require_permission(auth, Permission::ApiKeyRead)?;
        self.get_owned(auth, id).await
    }

    pub async fn get_me(&self, auth: &AuthDto) -> Result<ApiKeyResponse, ErrorResp> {
        if let Some(api_key) = &auth.api_key {
            self.get(auth, &Uuid::parse_str(&api_key.id).unwrap()).await
        } else {
            Err(ErrorResp::BadRequest("Not an API key session".to_string()))
        }
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &ApiKeyCreateReq,
    ) -> Result<ApiKeyCreateResp, ErrorResp> {
        require_permission(auth, Permission::ApiKeyCreate)?;

        let secret = random_bytes_as_text(32);
        let hashed = hash_sha256(&secret);

        let api_key = sqlx::query_as::<_, ApiKeyResponse>(
            r#"
                INSERT INTO api_key (name, key, "userId", permissions)
                VALUES ($1, $2, $3, $4)
                RETURNING id, name, "createdAt" as created_at, "updatedAt" as updated_at, permissions
            "#,
        )
        .bind(&dto.name)
        .bind(&hashed)
        .bind(auth.user.id)
        .bind(&dto.permissions)
        .fetch_one(&self.db.pool)
        .await?;

        Ok(ApiKeyCreateResp { api_key, secret })
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &ApiKeyUpdateReq,
    ) -> Result<ApiKeyResponse, ErrorResp> {
        require_permission(auth, Permission::ApiKeyUpdate)?;
        self.get_owned(auth, id).await?;

        sqlx::query_as::<_, ApiKeyResponse>(
            r#"
                UPDATE api_key
                SET name = COALESCE($1, name),
                    permissions = COALESCE($2, permissions)
                WHERE id = $3
                RETURNING id, name, "createdAt" as created_at, "updatedAt" as updated_at, permissions
            "#,
        )
        .bind(&dto.name)
        .bind(&dto.permissions)
        .bind(id)
        .fetch_one(&self.db.pool)
        .await
        .map_err(ErrorResp::from)
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::ApiKeyDelete)?;
        self.get_owned(auth, id).await?;
        sqlx::query(r#"DELETE FROM api_key WHERE id = $1"#)
            .bind(id)
            .execute(&self.db.pool)
            .await?;
        Ok(())
    }

    async fn get_owned(&self, auth: &AuthDto, id: &Uuid) -> Result<ApiKeyResponse, ErrorResp> {
        sqlx::query_as::<_, ApiKeyResponse>(
            r#"
                SELECT id, name, "createdAt" as created_at, "updatedAt" as updated_at, permissions
                FROM api_key
                WHERE id = $1 AND "userId" = $2
            "#,
        )
        .bind(id)
        .bind(auth.user.id)
        .fetch_optional(&self.db.pool)
        .await?
        .ok_or_else(|| ErrorResp::BadRequest("API Key not found".to_string()))
    }
}
