use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::tag::{
    create_tag_handler, delete_tag_handler, get_tag_handler, get_tags_handler, update_tag_handler,
};
use crate::handlers::stub;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tags", post(create_tag_handler))
        .route("/api/tags", get(get_tags_handler))
        .route("/api/tags", put(stub::not_implemented))
        .route("/api/tags/assets", put(stub::not_implemented))
        .route("/api/tags/{id}", get(get_tag_handler))
        .route("/api/tags/{id}", put(update_tag_handler))
        .route("/api/tags/{id}", axum::routing::patch(update_tag_handler))
        .route("/api/tags/{id}", delete(delete_tag_handler))
        .route("/api/tags/{id}/assets", put(stub::not_implemented))
        .route("/api/tags/{id}/assets", delete(stub::not_implemented))
}
