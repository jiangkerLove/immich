use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::face::{
    create_face_handler, delete_face_handler, get_faces_handler, reassign_face_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/faces", post(create_face_handler))
        .route("/api/faces", get(get_faces_handler))
        .route("/api/faces/{id}", put(reassign_face_handler))
        .route("/api/faces/{id}", delete(delete_face_handler))
}
