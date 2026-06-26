use axum::Router;
use axum::routing::{delete, get, put};

use crate::app_state::AppState;
use crate::handlers::queue::{
    empty_queue_handler, get_queue_handler, get_queue_jobs_handler, get_queues_handler,
    update_queue_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/queues", get(get_queues_handler))
        .route("/api/queues/{name}", get(get_queue_handler))
        .route("/api/queues/{name}", put(update_queue_handler))
        .route("/api/queues/{name}/jobs", get(get_queue_jobs_handler))
        .route("/api/queues/{name}/jobs", delete(empty_queue_handler))
}
