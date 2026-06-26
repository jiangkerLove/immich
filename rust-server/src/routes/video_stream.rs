use axum::Router;
use axum::routing::{delete, get};

use crate::app_state::AppState;
use crate::handlers::video_stream::{
    end_session_handler, get_main_playlist_handler, get_media_playlist_handler, get_segment_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/assets/{id}/video/stream/main.m3u8",
            get(get_main_playlist_handler),
        )
        .route(
            "/api/assets/{id}/video/stream/{sessionId}/{variantIndex}/playlist.m3u8",
            get(get_media_playlist_handler),
        )
        .route(
            "/api/assets/{id}/video/stream/{sessionId}/{variantIndex}/{filename}",
            get(get_segment_handler),
        )
        .route(
            "/api/assets/{id}/video/stream/{sessionId}",
            delete(end_session_handler),
        )
}
