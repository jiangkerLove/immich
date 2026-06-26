use axum::body::Body;
use axum::http::Response;
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::utils::response::json_response;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserLicenseResponse {
    pub license_key: String,
    pub activation_key: String,
    pub activated_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub profile_image_path: String,
    pub avatar_color: String,
    pub profile_changed_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAdminResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub profile_image_path: String,
    pub avatar_color: String,
    pub profile_changed_at: DateTime<Utc>,
    pub storage_label: Option<String>,
    pub should_change_password: bool,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub oauth_id: String,
    pub quota_size_in_bytes: Option<i64>,
    pub quota_usage_in_bytes: i64,
    pub status: String,
    pub license: Option<UserLicenseResponse>,
}

impl IntoResponse for UserAdminResponse {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}
