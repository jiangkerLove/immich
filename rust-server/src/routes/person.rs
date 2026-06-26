use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::person::{
    create_person_handler, delete_people_handler, delete_person_handler, get_people_handler,
    get_person_handler, get_person_statistics_handler, get_person_thumbnail_handler,
    merge_person_handler, reassign_faces_handler, update_people_handler, update_person_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/people", get(get_people_handler))
        .route("/api/people", post(create_person_handler))
        .route("/api/people", put(update_people_handler))
        .route("/api/people", delete(delete_people_handler))
        .route("/api/people/{id}", get(get_person_handler))
        .route("/api/people/{id}", put(update_person_handler))
        .route("/api/people/{id}", axum::routing::patch(update_person_handler))
        .route("/api/people/{id}", delete(delete_person_handler))
        .route(
            "/api/people/{id}/statistics",
            get(get_person_statistics_handler),
        )
        .route(
            "/api/people/{id}/thumbnail",
            get(get_person_thumbnail_handler),
        )
        .route("/api/people/{id}/reassign", put(reassign_faces_handler))
        .route("/api/people/{id}/merge", post(merge_person_handler))
}
