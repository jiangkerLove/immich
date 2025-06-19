use crate::app_state::AppState;
use crate::models::response::response::ErrorResp;
use axum::extract::State;
use axum::{Extension, Json};
use crate::models::request::auth::{LoginCredentialReq, LoginReq};
use crate::models::response::auth::LoginResp;

pub async fn login_handler(
    State(state): State<AppState>,
    Extension(login_details): Extension<LoginReq>,
    Json(login_credential): Json<LoginCredentialReq>,
) -> Result<LoginResp, ErrorResp> {
    state.auth_service.login(&login_credential, &login_details).await
}
