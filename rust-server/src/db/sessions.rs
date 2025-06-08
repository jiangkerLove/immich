use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use uuid::Uuid;


#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSession {
    pub token: String,
    pub device_os: String,
    pub device_type: String,
    pub user_id: Uuid,
}

impl NewSession {
    pub async fn insert(pool: &Pool<Postgres>, session: &NewSession) -> Result<(), sqlx::Error> {
        let _ = sqlx::query(
            r#"INSERT INTO sessions (token, "deviceOS", "deviceType", "userId") VALUES ($1, $2, $3, $4)"#
        )
            .bind(&session.token)
            .bind(&session.device_os)
            .bind(&session.device_type)
            .bind(&session.user_id)
            .execute(pool)
            .await?;

        Ok(())
    }
}
