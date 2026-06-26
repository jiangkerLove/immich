use axum::Router;
use axum::routing::{delete, get, post};

use crate::app_state::AppState;
use crate::handlers::sync::{
    delete_ack_handler, get_ack_handler, set_ack_handler, stream_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/sync/stream", post(stream_handler))
        .route("/api/sync/ack", get(get_ack_handler))
        .route("/api/sync/ack", post(set_ack_handler))
        .route("/api/sync/ack", delete(delete_ack_handler))
}
