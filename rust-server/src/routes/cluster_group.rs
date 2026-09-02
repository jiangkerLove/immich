use axum::Router;
use axum::routing::{delete, get, post};

use crate::app_state::AppState;
use crate::handlers::cluster_group::{
    accept_cluster_group_request_handler, create_cluster_group_request_handler,
    delete_cluster_group_request_handler, get_cluster_group_requests_for_group_handler,
    get_cluster_group_requests_handler, get_cluster_group_users_handler,
    leave_cluster_group_handler, regenerate_cluster_group_people_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/cluster-groups/requests",
            get(get_cluster_group_requests_handler),
        )
        .route(
            "/api/cluster-groups/requests/{id}/accept",
            post(accept_cluster_group_request_handler),
        )
        .route(
            "/api/cluster-groups/requests/{id}",
            delete(delete_cluster_group_request_handler),
        )
        .route(
            "/api/cluster-groups/{id}/requests",
            get(get_cluster_group_requests_for_group_handler)
                .put(create_cluster_group_request_handler),
        )
        .route(
            "/api/cluster-groups/{id}/users",
            get(get_cluster_group_users_handler),
        )
        .route(
            "/api/cluster-groups/{id}/regenerate-people",
            post(regenerate_cluster_group_people_handler),
        )
        .route(
            "/api/cluster-groups/{id}/leave",
            post(leave_cluster_group_handler),
        )
}
