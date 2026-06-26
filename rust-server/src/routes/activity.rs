use axum::Router;
use axum::routing::{delete, get, post};

use crate::app_state::AppState;
use crate::handlers::activity::{
    create_activity_handler, delete_activity_handler, get_activities_handler,
    get_activity_statistics_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/activities", get(get_activities_handler))
        .route("/api/activities", post(create_activity_handler))
        .route("/api/activities/statistics", get(get_activity_statistics_handler))
        .route("/api/activities/{id}", delete(delete_activity_handler))
}
