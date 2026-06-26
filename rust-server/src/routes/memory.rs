use axum::Router;
use axum::routing::{delete, get, patch, post, put};

use crate::app_state::AppState;
use crate::handlers::memory::{
    add_memory_assets_handler, create_memory_handler, delete_memory_handler,
    get_memory_handler, memory_statistics_handler, remove_memory_assets_handler,
    search_memories_handler, update_memory_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/memories", get(search_memories_handler))
        .route("/api/memories", post(create_memory_handler))
        .route("/api/memories/statistics", get(memory_statistics_handler))
        .route("/api/memories/{id}", get(get_memory_handler))
        .route("/api/memories/{id}", put(update_memory_handler))
        .route("/api/memories/{id}", patch(update_memory_handler))
        .route("/api/memories/{id}", delete(delete_memory_handler))
        .route("/api/memories/{id}/assets", put(add_memory_assets_handler))
        .route("/api/memories/{id}/assets", delete(remove_memory_assets_handler))
}
