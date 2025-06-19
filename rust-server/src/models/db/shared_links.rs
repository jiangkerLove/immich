use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct AuthSharedLinkDb {
    pub id: String,
    #[sqlx(default)]
    pub expires_at: Option<DateTime<Utc>>,
    pub user_id: String,
    pub show_exif: bool,
    pub allow_upload: bool,
    pub allow_download: bool,
    #[sqlx(default)]
    pub password: Option<String>,
}
