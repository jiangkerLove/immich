use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::tag::{
    bulk_tag_assets_handler, create_tag_handler, delete_tag_handler, get_tag_handler,
    get_tags_handler, tag_assets_handler, untag_assets_handler, update_tag_handler,
    upsert_tags_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tags", post(create_tag_handler))
        .route("/api/tags", get(get_tags_handler))
        .route("/api/tags", put(upsert_tags_handler))
        .route("/api/tags/assets", put(bulk_tag_assets_handler))
        .route("/api/tags/{id}", get(get_tag_handler))
        .route("/api/tags/{id}", put(update_tag_handler))
        .route("/api/tags/{id}", axum::routing::patch(update_tag_handler))
        .route("/api/tags/{id}", delete(delete_tag_handler))
        .route("/api/tags/{id}/assets", put(tag_assets_handler))
        .route("/api/tags/{id}/assets", delete(untag_assets_handler))
}
