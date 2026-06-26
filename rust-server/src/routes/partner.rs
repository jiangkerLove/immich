use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::partner::{
    create_partner_deprecated_handler, create_partner_handler, delete_partner_handler,
    get_partners_handler, update_partner_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/partners", get(get_partners_handler))
        .route("/api/partners", post(create_partner_handler))
        .route("/api/partners/{id}", post(create_partner_deprecated_handler))
        .route("/api/partners/{id}", put(update_partner_handler))
        .route("/api/partners/{id}", delete(delete_partner_handler))
}
