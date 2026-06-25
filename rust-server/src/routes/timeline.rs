use axum::Router;
use axum::routing::get;

use crate::app_state::AppState;
use crate::handlers::timeline::{get_time_bucket_handler, get_time_buckets_handler};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/timeline/buckets", get(get_time_buckets_handler))
        .route("/api/timeline/bucket", get(get_time_bucket_handler))
}
