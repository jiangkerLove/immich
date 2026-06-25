use axum::body::Body;
use axum::http::Response;
use axum::response::IntoResponse;
use serde::Serialize;

use crate::utils::response::json_response;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResp {
    pub access_token: String,
    pub user_id: uuid::Uuid,
    pub user_email: String,
    pub name: String,
    pub is_admin: bool,
    pub profile_image_path: String,
    pub should_change_password: bool,
    pub is_onboarded: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoutResp {
    pub successful: bool,
    pub redirect_uri: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateAccessTokenResp {
    pub auth_status: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatusResp {
    pub pin_code: bool,
    pub password: bool,
    pub is_elevated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_expires_at: Option<String>,
}

impl IntoResponse for ValidateAccessTokenResp {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}

impl IntoResponse for AuthStatusResp {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}
