use axum::extract::{Path, State};
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserResponse;
use crate::service::cluster_group::ClusterGroupRequestResponse;

pub async fn get_cluster_group_requests_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<ClusterGroupRequestResponse>>, ErrorResp> {
    Ok(Json(
        state.services.cluster_group.get_requests(&auth).await?,
    ))
}

pub async fn get_cluster_group_requests_for_group_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ClusterGroupRequestResponse>>, ErrorResp> {
    Ok(Json(
        state
            .services
            .cluster_group
            .get_requests_for_group(&auth, &id)
            .await?,
    ))
}

pub async fn get_cluster_group_users_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<UserResponse>>, ErrorResp> {
    Ok(Json(
        state.services.cluster_group.get_users(&auth, &id).await?,
    ))
}
