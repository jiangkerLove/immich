use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::db::DbService;
use crate::utils::crypto::{hash_sha256, random_bytes_as_text};
use crate::utils::permission::require_permission;
use crate::models::db::auth_permission::Permission;

#[derive(Clone)]
pub struct SessionService {
    db: DbService,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    pub id: Uuid,
    pub device_type: String,
    pub device_os: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub pin_expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_current: Option<bool>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreateReq {
    pub device_type: String,
    pub device_os: String,
    pub duration: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreateResp {
    #[serde(flatten)]
    pub session: SessionResponse,
    pub token: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateReq {
    pub is_pending_sync_reset: Option<bool>,
}

impl SessionService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            db: DbService::new(pool),
        }
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &SessionCreateReq,
    ) -> Result<SessionCreateResp, ErrorResp> {
        require_permission(auth, Permission::SessionCreate)?;
        let session_id = auth
            .session
            .as_ref()
            .ok_or_else(|| ErrorResp::BadRequest("This endpoint can only be used with a session token".to_string()))?
            .id
            .parse::<Uuid>()
            .map_err(|_| ErrorResp::BadRequest("Invalid session".to_string()))?;

        let token = random_bytes_as_text(32);
        let hashed = hash_sha256(&token);

        let expires_at: Option<DateTime<Utc>> = dto
            .duration
            .map(|seconds| Utc::now() + chrono::Duration::seconds(seconds));

        let row = sqlx::query_as::<_, SessionResponse>(
            r#"
                INSERT INTO session ("parentId", "userId", "expiresAt", "deviceType", "deviceOS", token)
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING
                    id,
                    "deviceType" as device_type,
                    "deviceOS" as device_os,
                    "createdAt" as created_at,
                    "updatedAt" as updated_at,
                    "expiresAt" as expires_at,
                    "pinExpiresAt" as pin_expires_at
            "#,
        )
        .bind(session_id)
        .bind(auth.user.id)
        .bind(expires_at)
        .bind(&dto.device_type)
        .bind(&dto.device_os)
        .bind(&hashed)
        .fetch_one(&self.db.pool)
        .await?;

        Ok(SessionCreateResp {
            session: row,
            token,
        })
    }

    pub async fn get_all(&self, auth: &AuthDto) -> Result<Vec<SessionResponse>, ErrorResp> {
        require_permission(auth, Permission::SessionRead)?;
        let current_id = auth.session.as_ref().map(|s| s.id.clone());

        let mut sessions = sqlx::query_as::<_, SessionResponse>(
            r#"
                SELECT
                    s.id,
                    s."deviceType" as device_type,
                    s."deviceOS" as device_os,
                    s."createdAt" as created_at,
                    s."updatedAt" as updated_at,
                    s."expiresAt" as expires_at,
                    s."pinExpiresAt" as pin_expires_at
                FROM session s
                INNER JOIN "user" u ON u.id = s."userId" AND u."deletedAt" IS NULL
                WHERE s."userId" = $1
                  AND (s."expiresAt" IS NULL OR s."expiresAt" > NOW())
                ORDER BY s."updatedAt" DESC, s."createdAt" DESC
            "#,
        )
        .bind(auth.user.id)
        .fetch_all(&self.db.pool)
        .await?;

        for session in &mut sessions {
            session.is_current = current_id
                .as_ref()
                .map(|id| id == &session.id.to_string());
        }

        Ok(sessions)
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &SessionUpdateReq,
    ) -> Result<SessionResponse, ErrorResp> {
        require_permission(auth, Permission::SessionUpdate)?;
        self.ensure_session_owner(auth, id).await?;

        if dto.is_pending_sync_reset.is_none() {
            return Err(ErrorResp::BadRequest("No fields to update".to_string()));
        }

        sqlx::query_as::<_, SessionResponse>(
            r#"
                UPDATE session
                SET "isPendingSyncReset" = COALESCE($1, "isPendingSyncReset")
                WHERE id = $2
                RETURNING
                    id,
                    "deviceType" as device_type,
                    "deviceOS" as device_os,
                    "createdAt" as created_at,
                    "updatedAt" as updated_at,
                    "expiresAt" as expires_at,
                    "pinExpiresAt" as pin_expires_at
            "#,
        )
        .bind(dto.is_pending_sync_reset)
        .bind(id)
        .fetch_one(&self.db.pool)
        .await
        .map_err(ErrorResp::from)
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::AuthDeviceDelete)?;
        self.ensure_session_owner(auth, id).await?;
        sqlx::query(r#"DELETE FROM session WHERE id = $1"#)
            .bind(id)
            .execute(&self.db.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_all(&self, auth: &AuthDto) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::SessionDelete)?;
        let exclude = auth
            .session
            .as_ref()
            .and_then(|s| Uuid::parse_str(&s.id).ok());

        if let Some(exclude_id) = exclude {
            sqlx::query(
                r#"DELETE FROM session WHERE "userId" = $1 AND id != $2"#,
            )
            .bind(auth.user.id)
            .bind(exclude_id)
            .execute(&self.db.pool)
            .await?;
        } else {
            sqlx::query(r#"DELETE FROM session WHERE "userId" = $1"#)
                .bind(auth.user.id)
                .execute(&self.db.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn lock(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::SessionLock)?;
        self.ensure_session_owner(auth, id).await?;
        sqlx::query(r#"UPDATE session SET "pinExpiresAt" = NULL WHERE id = $1"#)
            .bind(id)
            .execute(&self.db.pool)
            .await?;
        Ok(())
    }

    async fn ensure_session_owner(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        let owner: Option<Uuid> = sqlx::query_scalar(
            r#"SELECT "userId" FROM session WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        match owner {
            Some(user_id) if user_id == auth.user.id => Ok(()),
            Some(_) => Err(ErrorResp::BadRequest(
                "Not found or no session.update access".to_string(),
            )),
            None => Err(ErrorResp::BadRequest(
                "Not found or no session.update access".to_string(),
            )),
        }
    }
}
