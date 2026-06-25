use axum::Router;
use axum::routing::{get, post};

use crate::app_state::AppState;
use crate::handlers::asset_media::{
    bulk_upload_check_handler, download_original_handler, playback_video_handler,
    upload_asset_handler, view_thumbnail_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/assets", post(upload_asset_handler))
        .route("/api/assets/{id}/original", get(download_original_handler))
        .route("/api/assets/{id}/thumbnail", get(view_thumbnail_handler))
        .route("/api/assets/{id}/video/playback", get(playback_video_handler))
        .route("/api/assets/bulk-upload-check", post(bulk_upload_check_handler))
}
