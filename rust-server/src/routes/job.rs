use axum::Router;
use axum::routing::{get, post, put};

use crate::app_state::AppState;
use crate::handlers::job::{
    create_job_handler, get_jobs_legacy_handler, run_queue_command_legacy_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/jobs", get(get_jobs_legacy_handler))
        .route("/api/jobs", post(create_job_handler))
        .route("/api/jobs/{name}", put(run_queue_command_legacy_handler))
}
