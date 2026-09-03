use axum::Router;
use axum::routing::get;

use crate::app_state::AppState;
use crate::handlers::config::{
    get_public_config_defaults_handler, get_public_config_handler,
    get_user_config_defaults_handler, get_user_config_handler,
};

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/api/config", get(get_user_config_handler))
        .route("/api/config/defaults", get(get_user_config_defaults_handler))
}

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/api/public/config", get(get_public_config_handler))
        .route(
            "/api/public/config/defaults",
            get(get_public_config_defaults_handler),
        )
}
