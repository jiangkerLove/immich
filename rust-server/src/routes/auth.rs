use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::auth::{
    admin_sign_up_handler, auth_status_handler, change_password_handler, change_pin_code_handler,
    lock_session_handler, login_handler, logout_handler, reset_pin_code_handler,
    setup_pin_code_handler, unlink_all_oauth_handler, unlock_session_handler,
    validate_token_handler,
};

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/admin-sign-up", post(admin_sign_up_handler))
}

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/logout", post(logout_handler))
        .route("/api/auth/validateToken", post(validate_token_handler))
        .route("/api/auth/status", get(auth_status_handler))
        .route("/api/auth/change-password", post(change_password_handler))
        .route("/api/auth/pin-code", post(setup_pin_code_handler))
        .route("/api/auth/pin-code", put(change_pin_code_handler))
        .route("/api/auth/pin-code", delete(reset_pin_code_handler))
        .route("/api/auth/session/unlock", post(unlock_session_handler))
        .route("/api/auth/session/lock", post(lock_session_handler))
        // Auth admin
        .route("/api/admin/auth/unlink-all", post(unlink_all_oauth_handler))
}
