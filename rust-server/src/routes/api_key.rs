use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::api_key::{
    create_api_key_handler, delete_api_key_handler, get_api_key_handler,
    get_api_key_me_handler, get_api_keys_handler, rotate_api_key_handler, update_api_key_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/api-keys", post(create_api_key_handler))
        .route("/api/api-keys", get(get_api_keys_handler))
        .route("/api/api-keys/me", get(get_api_key_me_handler))
        .route("/api/api-keys/{id}", get(get_api_key_handler))
        .route("/api/api-keys/{id}", put(update_api_key_handler))
        .route("/api/api-keys/{id}", axum::routing::patch(update_api_key_handler))
        .route("/api/api-keys/{id}/rotate", post(rotate_api_key_handler))
        .route("/api/api-keys/{id}", delete(delete_api_key_handler))
}
