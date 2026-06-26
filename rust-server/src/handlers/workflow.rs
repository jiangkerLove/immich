use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::workflow::{
    WorkflowCreateReq, WorkflowResponse, WorkflowSearchQuery, WorkflowShareResponse,
    WorkflowUpdateReq,
};
use crate::utils::workflow::WorkflowTriggerResponse;

pub async fn create_workflow_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<WorkflowCreateReq>,
) -> Result<Json<WorkflowResponse>, ErrorResp> {
    Ok(Json(state.services.workflow.create(&auth, &dto).await?))
}

pub async fn search_workflows_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<WorkflowSearchQuery>,
) -> Result<Json<Vec<WorkflowResponse>>, ErrorResp> {
    Ok(Json(state.services.workflow.search(&auth, &query).await?))
}

pub async fn get_workflow_triggers_handler(
    State(state): State<AppState>,
) -> Json<Vec<WorkflowTriggerResponse>> {
    Json(state.services.workflow.get_triggers())
}

pub async fn get_workflow_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowResponse>, ErrorResp> {
    Ok(Json(state.services.workflow.get(&auth, &id).await?))
}

pub async fn get_workflow_share_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowShareResponse>, ErrorResp> {
    Ok(Json(state.services.workflow.share(&auth, &id).await?))
}

pub async fn update_workflow_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<WorkflowUpdateReq>,
) -> Result<Json<WorkflowResponse>, ErrorResp> {
    Ok(Json(
        state.services.workflow.update(&auth, &id, &dto).await?,
    ))
}

pub async fn patch_workflow_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<WorkflowUpdateReq>,
) -> Result<Json<WorkflowResponse>, ErrorResp> {
    Ok(Json(
        state.services.workflow.update(&auth, &id, &dto).await?,
    ))
}

pub async fn delete_workflow_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.workflow.delete(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
