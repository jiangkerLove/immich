use axum::Router;
use axum::routing::{delete, get, put};

use crate::app_state::AppState;
use crate::handlers::server::{
    about_handler, apk_links_handler, config_handler, custom_css_handler, delete_server_license_handler,
    features_handler, get_server_license_handler, media_types_handler, ping_handler,
    set_server_license_handler, statistics_handler, storage_handler, version_check_handler,
    version_handler, version_history_handler, well_known_handler,
};

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/api/server/ping", get(ping_handler))
        .route("/api/server/version", get(version_handler))
        .route("/api/server/features", get(features_handler))
        .route("/api/server/config", get(config_handler))
        .route("/api/server/media-types", get(media_types_handler))
        .route("/api/server/version-history", get(version_history_handler))
        .route("/.well-known/immich", get(well_known_handler))
        .route("/custom.css", get(custom_css_handler))
}

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/api/server/about", get(about_handler))
        .route("/api/server/apk-links", get(apk_links_handler))
        .route("/api/server/storage", get(storage_handler))
        .route("/api/server/statistics", get(statistics_handler))
        .route("/api/server/license", get(get_server_license_handler))
        .route("/api/server/license", put(set_server_license_handler))
        .route("/api/server/license", delete(delete_server_license_handler))
        .route("/api/server/version-check", get(version_check_handler))
}
