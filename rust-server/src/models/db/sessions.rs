use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;


#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSession {
    pub token: String,
    pub device_os: String,
    pub device_type: String,
    pub user_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub id: String,
    pub has_elevated_permission: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionPO {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub user_id: Uuid,
    pub device_os: String,
    pub device_type: String,
    pub pin_expires_at: Option<DateTime<Utc>>,
}

impl NewSession {
    pub async fn insert(&self, pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
        let _ = sqlx::query(
            r#"INSERT INTO sessions (token, "deviceOS", "deviceType", "userId") VALUES ($1, $2, $3, $4)"#
        )
            .bind(&self.token)
            .bind(&self.device_os)
            .bind(&self.device_type)
            .bind(&self.user_id)
            .execute(pool)
            .await?;

        Ok(())
    }
}

impl SessionPO {
    pub async fn query_by_token(pool: &Pool<Postgres>, token: &String) -> Result<Option<SessionPO>, sqlx::Error> {
        let session = sqlx::query_as::<_, Self>(
            r#"
                    SELECT
                        id,
                        "createdAt" as "created_at",
                        "updatedAt" as "updated_at",
                        "expiresAt" as "expires_at",
                        "userId" as "user_id",
                        "deviceOS" as "device_os",
                        "deviceType" as "device_type",
                        "pinExpiresAt" as "pin_expires_at"
                    FROM sessions
                    WHERE token = $1
                "#,
        ).bind(token).fetch_optional(pool).await?;
        Ok(session)
    }
}
