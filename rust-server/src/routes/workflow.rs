use axum::Router;
use axum::routing::{delete, get, patch, post, put};

use crate::app_state::AppState;
use crate::handlers::workflow::{
    create_workflow_handler, delete_workflow_handler, get_workflow_handler,
    get_workflow_share_handler, get_workflow_triggers_handler, patch_workflow_handler,
    search_workflows_handler, update_workflow_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/workflows", post(create_workflow_handler))
        .route("/api/workflows", get(search_workflows_handler))
        .route("/api/workflows/triggers", get(get_workflow_triggers_handler))
        .route("/api/workflows/{id}", get(get_workflow_handler))
        .route("/api/workflows/{id}/share", get(get_workflow_share_handler))
        .route("/api/workflows/{id}", put(update_workflow_handler))
        .route("/api/workflows/{id}", patch(patch_workflow_handler))
        .route("/api/workflows/{id}", delete(delete_workflow_handler))
}
