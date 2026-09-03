use axum::Router;
use axum::routing::{delete, get};

use crate::app_state::AppState;
use crate::handlers::asset_file::{
    delete_asset_file_handler, download_asset_file_handler, get_asset_file_handler,
    search_asset_files_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/asset-files", get(search_asset_files_handler))
        .route("/api/asset-files/{id}", get(get_asset_file_handler))
        .route("/api/asset-files/{id}/download", get(download_asset_file_handler))
        .route("/api/asset-files/{id}", delete(delete_asset_file_handler))
}
