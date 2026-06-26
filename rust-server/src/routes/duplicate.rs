use axum::Router;
use axum::routing::{delete, get, post};

use crate::app_state::AppState;
use crate::handlers::duplicate::{
    delete_duplicate_handler, delete_duplicates_handler, get_duplicates_handler,
    resolve_duplicates_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/duplicates", get(get_duplicates_handler))
        .route("/api/duplicates", delete(delete_duplicates_handler))
        .route("/api/duplicates/{id}", delete(delete_duplicate_handler))
        .route("/api/duplicates/resolve", post(resolve_duplicates_handler))
}
