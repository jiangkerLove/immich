use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserResponse;
use crate::service::cluster_group::{
    ClusterGroupRequestCreateReq, ClusterGroupRequestResponse,
};

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

pub async fn create_cluster_group_request_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<ClusterGroupRequestCreateReq>,
) -> Result<(StatusCode, Json<ClusterGroupRequestResponse>), ErrorResp> {
    let result = state
        .services
        .cluster_group
        .create_request(&auth, &id, &dto.user_id)
        .await?;
    let status = if result.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(result.value)))
}

pub async fn accept_cluster_group_request_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .cluster_group
        .accept_request(&auth, &id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_cluster_group_request_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .cluster_group
        .delete_request(&auth, &id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn regenerate_cluster_group_people_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .cluster_group
        .regenerate_people(&auth, &id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn leave_cluster_group_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.cluster_group.leave(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
