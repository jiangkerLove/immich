use axum::body::Body;
use axum::extract::State;
use axum::http::{Response, StatusCode};
use axum::{Extension, Json};

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::request::auth::{
    ChangePasswordReq, LoginCredentialReq, LoginReq, PinCodeChangeReq, PinCodeResetReq,
    PinCodeSetupReq, SessionUnlockReq, SignUpReq,
};
use crate::models::response::auth::{
    AuthStatusResp, ValidateAccessTokenResp,
};
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::service::auth::AuthService;
use crate::utils::headers::get_auth_type;
use crate::utils::response::{respond_with_auth_cookies, respond_without_auth_cookies};

pub async fn login_handler(
    State(state): State<AppState>,
    Extension(login_details): Extension<LoginReq>,
    Json(login_credential): Json<LoginCredentialReq>,
) -> Result<Response<Body>, ErrorResp> {
    let body = state
        .services
        .auth
        .login(&login_credential, &login_details)
        .await?;
    Ok(respond_with_auth_cookies(
        &body,
        login_details.is_secure,
        &body.access_token,
        "password",
    ))
}

pub async fn admin_sign_up_handler(
    State(state): State<AppState>,
    Json(dto): Json<SignUpReq>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.auth.admin_sign_up(&dto).await
}

pub async fn logout_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    headers: axum::http::HeaderMap,
) -> Result<Response<Body>, ErrorResp> {
    let _auth_type = get_auth_type(&headers);
    let body = state.services.auth.logout(&auth).await?;
    Ok(respond_without_auth_cookies(&body))
}

pub async fn validate_token_handler() -> ValidateAccessTokenResp {
    AuthService::validate_access_token()
}

pub async fn auth_status_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<AuthStatusResp, ErrorResp> {
    state.services.auth.get_auth_status(&auth).await
}

pub async fn change_password_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<ChangePasswordReq>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.auth.change_password(&auth, &dto).await
}

pub async fn setup_pin_code_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<PinCodeSetupReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.auth.setup_pin_code(&auth, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn change_pin_code_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<PinCodeChangeReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.auth.change_pin_code(&auth, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reset_pin_code_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<PinCodeResetReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.auth.reset_pin_code(&auth, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unlock_session_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<SessionUnlockReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.auth.unlock_session(&auth, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn lock_session_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<StatusCode, ErrorResp> {
    state.services.auth.lock_session(&auth).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unlink_all_oauth_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<StatusCode, ErrorResp> {
    state.services.auth_admin.unlink_all(&auth).await?;
    Ok(StatusCode::NO_CONTENT)
}
