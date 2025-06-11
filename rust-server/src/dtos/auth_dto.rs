use axum::body::Body;
use axum::http::Response;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::db::api_key::AuthApiKey;
use crate::db::sessions::AuthSession;
use crate::db::shared_links::AuthSharedLink;
use crate::db::users::AuthUser;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LoginDetails {
    pub is_secure: bool,
    pub client_ip: String,
    pub device_type: String,
    pub device_os: String,
}

#[derive(Serialize, Deserialize)]
pub struct LoginCredentialDto {
    pub email: String,
    pub password: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponseDto {
    pub access_token: String,
    pub user_id: Uuid,
    pub user_email: String,
    pub name: String,
    pub is_admin: bool,
    pub profile_image_path: String,
    pub should_change_password: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthDto {
    pub user: AuthUser,
    pub api_key: Option<AuthApiKey>,
    pub session: Option<AuthSession>,
    pub shared_link: Option<AuthSharedLink>,
}

impl IntoResponse for LoginResponseDto {
    fn into_response(self) -> Response<Body> {
        Response::new(Body::from(serde_json::to_string(&self).unwrap()))
    }
}