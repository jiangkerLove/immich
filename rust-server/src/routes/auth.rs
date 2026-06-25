use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::auth::{
    admin_sign_up_handler, auth_status_handler, login_handler, logout_handler,
    validate_token_handler,
};
use crate::handlers::stub;

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/admin-sign-up", post(admin_sign_up_handler))
        // Maintenance public
        .route("/api/admin/maintenance/status", get(stub::not_implemented))
        .route("/api/admin/maintenance/login", post(stub::not_implemented))
        .route("/api/admin/database-backups/start-restore", post(stub::not_implemented))
}

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/logout", post(logout_handler))
        .route("/api/auth/validateToken", post(validate_token_handler))
        .route("/api/auth/status", get(auth_status_handler))
        .route("/api/auth/change-password", post(stub::not_implemented))
        .route("/api/auth/pin-code", post(stub::not_implemented))
        .route("/api/auth/pin-code", put(stub::not_implemented))
        .route("/api/auth/pin-code", delete(stub::not_implemented))
        .route("/api/auth/session/unlock", post(stub::not_implemented))
        .route("/api/auth/session/lock", post(stub::not_implemented))
        // Auth admin
        .route("/api/admin/auth/unlink-all", post(stub::not_implemented))
}
