use axum::Router;
use axum::routing::{delete, get, patch, post, put};

use crate::app_state::AppState;
use crate::handlers::library::{
    create_library_handler, delete_library_handler, get_libraries_handler, get_library_handler,
    get_library_statistics_handler, patch_library_handler, scan_library_handler,
    update_library_handler, validate_library_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/libraries", get(get_libraries_handler))
        .route("/api/libraries", post(create_library_handler))
        .route("/api/libraries/{id}", get(get_library_handler))
        .route("/api/libraries/{id}", put(update_library_handler))
        .route("/api/libraries/{id}", patch(patch_library_handler))
        .route("/api/libraries/{id}", delete(delete_library_handler))
        .route("/api/libraries/{id}/validate", post(validate_library_handler))
        .route(
            "/api/libraries/{id}/statistics",
            get(get_library_statistics_handler),
        )
        .route("/api/libraries/{id}/scan", post(scan_library_handler))
}
