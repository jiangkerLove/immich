use axum::Router;
use axum::routing::get;

use crate::app_state::AppState;
use crate::handlers::view::{
    get_assets_by_original_path_handler, get_unique_original_paths_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/view/folder/unique-paths",
            get(get_unique_original_paths_handler),
        )
        .route("/api/view/folder", get(get_assets_by_original_path_handler))
}
