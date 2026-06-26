use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::stack::{
    create_stack_handler, delete_stack_handler, delete_stacks_handler, get_stack_handler,
    remove_asset_from_stack_handler, search_stacks_handler, update_stack_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/stacks", get(search_stacks_handler))
        .route("/api/stacks", post(create_stack_handler))
        .route("/api/stacks", delete(delete_stacks_handler))
        .route("/api/stacks/{id}", get(get_stack_handler))
        .route("/api/stacks/{id}", put(update_stack_handler))
        .route("/api/stacks/{id}", axum::routing::patch(update_stack_handler))
        .route("/api/stacks/{id}", delete(delete_stack_handler))
        .route(
            "/api/stacks/{id}/assets/{assetId}",
            delete(remove_asset_from_stack_handler),
        )
}
