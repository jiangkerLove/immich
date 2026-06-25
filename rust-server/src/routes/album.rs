use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::album::{
    create_album_handler, get_album_handler, get_album_statistics_handler, get_albums_handler,
};
use crate::handlers::stub;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/albums", get(get_albums_handler))
        .route("/api/albums", post(create_album_handler))
        .route("/api/albums/statistics", get(get_album_statistics_handler))
        .route("/api/albums/{id}", get(get_album_handler))
        .route("/api/albums/{id}", axum::routing::patch(stub::not_implemented))
        .route("/api/albums/{id}", delete(stub::not_implemented))
        .route("/api/albums/{id}/map-markers", get(stub::not_implemented))
        .route("/api/albums/{id}/assets", put(stub::not_implemented))
        .route("/api/albums/assets", put(stub::not_implemented))
        .route("/api/albums/{id}/assets", delete(stub::not_implemented))
        .route("/api/albums/{id}/users", put(stub::not_implemented))
        .route("/api/albums/{id}/user/{userId}", put(stub::not_implemented))
        .route("/api/albums/{id}/user/{userId}", delete(stub::not_implemented))
}
