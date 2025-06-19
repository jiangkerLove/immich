use axum::body::Body;
use axum::http::Response;
use axum::response::IntoResponse;
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResp {
    pub access_token: String,
    pub user_id: Uuid,
    pub user_email: String,
    pub name: String,
    pub is_admin: bool,
    pub profile_image_path: String,
    pub should_change_password: bool,
}

impl IntoResponse for LoginResp {
    fn into_response(self) -> Response<Body> {
        Response::new(Body::from(serde_json::to_string(&self).unwrap()))
    }
}