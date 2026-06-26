use axum::Router;
use axum::routing::post;

use crate::app_state::AppState;
use crate::handlers::download::{download_archive_handler, get_download_info_handler};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/download/info", post(get_download_info_handler))
        .route("/api/download/archive", post(download_archive_handler))
}
