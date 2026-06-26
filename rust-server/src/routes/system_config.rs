use axum::Router;
use axum::routing::{get, put};

use crate::app_state::AppState;
use crate::handlers::system_config::{
    get_storage_template_options_handler, get_system_config_defaults_handler,
    get_system_config_handler, update_system_config_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/system-config", get(get_system_config_handler))
        .route("/api/system-config/defaults", get(get_system_config_defaults_handler))
        .route("/api/system-config", put(update_system_config_handler))
        .route(
            "/api/system-config/storage-template-options",
            get(get_storage_template_options_handler),
        )
}
