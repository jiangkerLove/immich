use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSession {
    pub token: Vec<u8>,
    pub device_os: String,
    pub device_type: String,
    pub user_id: Uuid,
    pub oauth_sid: Option<String>,
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
        sqlx::query(
            r#"INSERT INTO session (token, "deviceOS", "deviceType", "userId", "oauthSid") VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(&self.token)
        .bind(&self.device_os)
        .bind(&self.device_type)
        .bind(&self.user_id)
        .bind(&self.oauth_sid)
        .execute(pool)
        .await?;

        Ok(())
    }
}

impl SessionPO {
    pub async fn query_by_token(
        pool: &Pool<Postgres>,
        token: &[u8],
    ) -> Result<Option<SessionPO>, sqlx::Error> {
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
                FROM session
                WHERE token = $1
                  AND ("expiresAt" IS NULL OR "expiresAt" > NOW())
            "#,
        )
        .bind(token)
        .fetch_optional(pool)
        .await?;
        Ok(session)
    }

    pub async fn get_by_id(
        pool: &Pool<Postgres>,
        id: &Uuid,
    ) -> Result<Option<SessionPO>, sqlx::Error> {
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
                FROM session
                WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(session)
    }

    pub async fn delete(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM session WHERE id = $1"#)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn invalidate_oauth(
        pool: &Pool<Postgres>,
        oauth_sid: Option<&str>,
        oauth_id: Option<&str>,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        match (oauth_sid, oauth_id) {
            (Some(sid), Some(sub)) => sqlx::query_scalar(
                r#"
                    DELETE FROM session
                    USING "user"
                    WHERE session."userId" = "user".id
                      AND session."oauthSid" = $1
                      AND "user"."oauthId" = $2
                    RETURNING session.id
                "#,
            )
            .bind(sid)
            .bind(sub)
            .fetch_all(pool)
            .await,
            (None, Some(sub)) => sqlx::query_scalar(
                r#"
                    DELETE FROM session
                    USING "user"
                    WHERE session."userId" = "user".id
                      AND "user"."oauthId" = $1
                    RETURNING session.id
                "#,
            )
            .bind(sub)
            .fetch_all(pool)
            .await,
            (Some(sid), None) => sqlx::query_scalar(
                r#"DELETE FROM session WHERE "oauthSid" = $1 RETURNING id"#,
            )
            .bind(sid)
            .fetch_all(pool)
            .await,
            (None, None) => Ok(vec![]),
        }
    }
}
