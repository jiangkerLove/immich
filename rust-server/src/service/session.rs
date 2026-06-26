use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::db::DbService;
use crate::service::websocket::WebSocketHub;
use crate::utils::crypto::{hash_sha256, random_bytes_as_text};
use crate::utils::permission::require_permission;
use crate::models::db::auth_permission::Permission;

#[derive(Clone)]
pub struct SessionService {
    db: DbService,
    websocket: WebSocketHub,
}

#[derive(Debug, FromRow)]
struct SessionRow {
    id: Uuid,
    device_type: String,
    device_os: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    app_version: Option<String>,
    is_pending_sync_reset: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    pub id: Uuid,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub current: bool,
    pub device_type: String,
    pub device_os: String,
    pub app_version: Option<String>,
    pub is_pending_sync_reset: bool,
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

const SESSION_SELECT: &str = r#"
    id,
    "deviceType" as device_type,
    "deviceOS" as device_os,
    "createdAt" as created_at,
    "updatedAt" as updated_at,
    "expiresAt" as expires_at,
    "appVersion" as app_version,
    "isPendingSyncReset" as is_pending_sync_reset
"#;

fn format_datetime(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub async fn list_sessions_for_user(
    pool: &sqlx::PgPool,
    user_id: &Uuid,
) -> Result<Vec<SessionResponse>, ErrorResp> {
    let rows = sqlx::query_as::<_, SessionRow>(&format!(
        r#"
            SELECT {SESSION_SELECT}
            FROM session s
            INNER JOIN "user" u ON u.id = s."userId" AND u."deletedAt" IS NULL
            WHERE s."userId" = $1
              AND (s."expiresAt" IS NULL OR s."expiresAt" > NOW())
            ORDER BY s."updatedAt" DESC, s."createdAt" DESC
        "#
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| map_session(row, None))
        .collect())
}

fn map_session(row: SessionRow, current_id: Option<&str>) -> SessionResponse {
    SessionResponse {
        id: row.id,
        created_at: format_datetime(&row.created_at),
        updated_at: format_datetime(&row.updated_at),
        expires_at: row.expires_at.as_ref().map(format_datetime),
        current: current_id.is_some_and(|id| id == row.id.to_string()),
        device_type: row.device_type,
        device_os: row.device_os,
        app_version: row.app_version,
        is_pending_sync_reset: row.is_pending_sync_reset,
    }
}

impl SessionService {
    pub fn new(pool: sqlx::PgPool, websocket: WebSocketHub) -> Self {
        Self {
            db: DbService::new(pool),
            websocket,
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

        let row = sqlx::query_as::<_, SessionRow>(&format!(
            r#"
                INSERT INTO session ("parentId", "userId", "expiresAt", "deviceType", "deviceOS", token)
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING {SESSION_SELECT}
            "#
        ))
        .bind(session_id)
        .bind(auth.user.id)
        .bind(expires_at)
        .bind(&dto.device_type)
        .bind(&dto.device_os)
        .bind(&hashed)
        .fetch_one(&self.db.pool)
        .await?;

        Ok(SessionCreateResp {
            session: map_session(row, Some(&session_id.to_string())),
            token,
        })
    }

    pub async fn get_all(&self, auth: &AuthDto) -> Result<Vec<SessionResponse>, ErrorResp> {
        require_permission(auth, Permission::SessionRead)?;
        let current_id = auth.session.as_ref().map(|s| s.id.as_str());

        let rows = sqlx::query_as::<_, SessionRow>(&format!(
            r#"
                SELECT {SESSION_SELECT}
                FROM session s
                INNER JOIN "user" u ON u.id = s."userId" AND u."deletedAt" IS NULL
                WHERE s."userId" = $1
                  AND (s."expiresAt" IS NULL OR s."expiresAt" > NOW())
                ORDER BY s."updatedAt" DESC, s."createdAt" DESC
            "#
        ))
        .bind(auth.user.id)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| map_session(row, current_id))
            .collect())
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

        let current_id = auth.session.as_ref().map(|s| s.id.as_str());

        let row = sqlx::query_as::<_, SessionRow>(&format!(
            r#"
                UPDATE session
                SET "isPendingSyncReset" = COALESCE($1, "isPendingSyncReset")
                WHERE id = $2
                RETURNING {SESSION_SELECT}
            "#
        ))
        .bind(dto.is_pending_sync_reset)
        .bind(id)
        .fetch_one(&self.db.pool)
        .await
        .map_err(ErrorResp::from)?;

        Ok(map_session(row, current_id))
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::AuthDeviceDelete)?;
        self.ensure_session_owner(auth, id).await?;
        sqlx::query(r#"DELETE FROM session WHERE id = $1"#)
            .bind(id)
            .execute(&self.db.pool)
            .await?;
        self.websocket.emit_session_delete(*id);
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
