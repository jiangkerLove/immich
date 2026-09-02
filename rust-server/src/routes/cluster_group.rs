use axum::Router;
use axum::routing::get;

use crate::app_state::AppState;
use crate::handlers::cluster_group::{
    get_cluster_group_requests_for_group_handler, get_cluster_group_requests_handler,
    get_cluster_group_users_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/cluster-groups/requests",
            get(get_cluster_group_requests_handler),
        )
        .route(
            "/api/cluster-groups/{id}/requests",
            get(get_cluster_group_requests_for_group_handler),
        )
        .route(
            "/api/cluster-groups/{id}/users",
            get(get_cluster_group_users_handler),
        )
}
