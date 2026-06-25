use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::session::{
    create_session_handler, delete_all_sessions_handler, delete_session_handler,
    get_sessions_handler, lock_session_handler, update_session_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/sessions", post(create_session_handler))
        .route("/api/sessions", get(get_sessions_handler))
        .route("/api/sessions", delete(delete_all_sessions_handler))
        .route("/api/sessions/{id}", put(update_session_handler))
        .route("/api/sessions/{id}", axum::routing::patch(update_session_handler))
        .route("/api/sessions/{id}", delete(delete_session_handler))
        .route("/api/sessions/{id}/lock", post(lock_session_handler))
}
