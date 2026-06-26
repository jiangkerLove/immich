use axum::Router;
use axum::routing::get;

use crate::app_state::AppState;
use crate::handlers::plugin::{
    get_plugin_handler, search_plugin_methods_handler, search_plugins_handler,
    search_plugin_templates_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/plugins", get(search_plugins_handler))
        .route("/api/plugins/methods", get(search_plugin_methods_handler))
        .route("/api/plugins/templates", get(search_plugin_templates_handler))
        .route("/api/plugins/{id}", get(get_plugin_handler))
}
