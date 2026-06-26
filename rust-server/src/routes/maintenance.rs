use axum::Router;
use axum::routing::{get, post};

use crate::app_state::AppState;
use crate::handlers::maintenance::{
    detect_prior_install_handler, maintenance_login_handler, maintenance_status_handler,
    set_maintenance_mode_handler,
};

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/maintenance/status",
            get(maintenance_status_handler),
        )
        .route("/api/admin/maintenance/login", post(maintenance_login_handler))
}

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/maintenance/detect-install",
            get(detect_prior_install_handler),
        )
        .route("/api/admin/maintenance", post(set_maintenance_mode_handler))
}
