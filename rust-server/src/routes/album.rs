use axum::Router;
use axum::routing::{delete, get, patch, post, put};

use crate::app_state::AppState;
use crate::handlers::album::{
    add_assets_to_album_handler, add_assets_to_albums_handler, add_users_to_album_handler,
    create_album_handler, delete_album_handler, get_album_handler, get_album_map_markers_handler,
    get_album_statistics_handler, get_albums_handler, remove_assets_from_album_handler,
    remove_user_from_album_handler, update_album_handler, update_album_user_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/albums", get(get_albums_handler))
        .route("/api/albums", post(create_album_handler))
        .route("/api/albums/statistics", get(get_album_statistics_handler))
        .route("/api/albums/{id}", get(get_album_handler))
        .route("/api/albums/{id}", patch(update_album_handler))
        .route("/api/albums/{id}", delete(delete_album_handler))
        .route("/api/albums/{id}/map-markers", get(get_album_map_markers_handler))
        .route("/api/albums/{id}/assets", put(add_assets_to_album_handler))
        .route("/api/albums/assets", put(add_assets_to_albums_handler))
        .route("/api/albums/{id}/assets", delete(remove_assets_from_album_handler))
        .route("/api/albums/{id}/users", put(add_users_to_album_handler))
        .route(
            "/api/albums/{id}/user/{userId}",
            put(update_album_user_handler),
        )
        .route(
            "/api/albums/{id}/user/{userId}",
            delete(remove_user_from_album_handler),
        )
}
