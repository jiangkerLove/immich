use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::stack::{
    BulkIdsReq, StackCreateReq, StackResponse, StackSearchQuery, StackUpdateReq,
};

pub async fn search_stacks_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<StackSearchQuery>,
) -> Result<Json<Vec<StackResponse>>, ErrorResp> {
    Ok(Json(state.services.stack.search(&auth, &query).await?))
}

pub async fn create_stack_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<StackCreateReq>,
) -> Result<Json<StackResponse>, ErrorResp> {
    Ok(Json(state.services.stack.create(&auth, &dto).await?))
}

pub async fn delete_stacks_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<BulkIdsReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.stack.delete_all(&auth, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_stack_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<StackResponse>, ErrorResp> {
    Ok(Json(state.services.stack.get(&auth, &id).await?))
}

pub async fn update_stack_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<StackUpdateReq>,
) -> Result<Json<StackResponse>, ErrorResp> {
    Ok(Json(state.services.stack.update(&auth, &id, &dto).await?))
}

pub async fn delete_stack_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.stack.delete(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_asset_from_stack_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path((id, asset_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .stack
        .remove_asset(&auth, &id, &asset_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
