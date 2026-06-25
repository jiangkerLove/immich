use axum::body::Body;
use axum::http::Response;
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::utils::response::json_response;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAdminResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub profile_image_path: String,
    pub avatar_color: String,
    pub profile_changed_at: DateTime<Utc>,
    pub storage_label: String,
    pub should_change_password: bool,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub oauth_id: String,
    pub quota_size_in_bytes: Option<i64>,
    pub quota_usage_in_bytes: i64,
    pub status: String,
    pub license: Option<String>,
}

impl IntoResponse for UserAdminResponse {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}
