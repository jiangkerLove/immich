use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::shared_link::{
    add_shared_link_assets_handler, create_shared_link_handler, delete_shared_link_handler,
    get_my_shared_link_handler, get_shared_link_handler, get_shared_links_handler,
    remove_shared_link_assets_handler, shared_link_login_handler, update_shared_link_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/shared-links", get(get_shared_links_handler))
        .route("/api/shared-links/login", post(shared_link_login_handler))
        .route("/api/shared-links/me", get(get_my_shared_link_handler))
        .route("/api/shared-links/{id}", get(get_shared_link_handler))
        .route("/api/shared-links", post(create_shared_link_handler))
        .route("/api/shared-links/{id}", axum::routing::patch(update_shared_link_handler))
        .route("/api/shared-links/{id}", delete(delete_shared_link_handler))
        .route("/api/shared-links/{id}/assets", put(add_shared_link_assets_handler))
        .route("/api/shared-links/{id}/assets", delete(remove_shared_link_assets_handler))
}
