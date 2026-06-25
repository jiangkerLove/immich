use axum::Router;
use axum::routing::get;

use crate::app_state::AppState;
use crate::handlers::memory::search_memories_handler;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/memories", get(search_memories_handler))
}
