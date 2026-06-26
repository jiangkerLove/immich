use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::memory::{MemoryResponse, MemoryStatisticsResponse};
use crate::models::response::response::ErrorResp;
use crate::service::album::{BulkIdResponse, BulkIdsReq};
use crate::service::memory::{MemoryCreateReq, MemorySearchQuery, MemoryUpdateReq};

pub async fn search_memories_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<MemorySearchQuery>,
) -> Result<Json<Vec<MemoryResponse>>, ErrorResp> {
    Ok(Json(
        state.services.memory.search(&auth, &query).await?,
    ))
}

pub async fn create_memory_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<MemoryCreateReq>,
) -> Result<(StatusCode, Json<MemoryResponse>), ErrorResp> {
    Ok((
        StatusCode::CREATED,
        Json(state.services.memory.create(&auth, &dto).await?),
    ))
}

pub async fn memory_statistics_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<MemorySearchQuery>,
) -> Result<Json<MemoryStatisticsResponse>, ErrorResp> {
    Ok(Json(
        state.services.memory.statistics(&auth, &query).await?,
    ))
}

pub async fn get_memory_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<MemoryResponse>, ErrorResp> {
    Ok(Json(state.services.memory.get(&auth, &id).await?))
}

pub async fn update_memory_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<MemoryUpdateReq>,
) -> Result<Json<MemoryResponse>, ErrorResp> {
    Ok(Json(state.services.memory.update(&auth, &id, &dto).await?))
}

pub async fn delete_memory_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.memory.delete(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn add_memory_assets_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<BulkIdsReq>,
) -> Result<Json<Vec<BulkIdResponse>>, ErrorResp> {
    Ok(Json(
        state.services.memory.add_assets(&auth, &id, &dto).await?,
    ))
}

pub async fn remove_memory_assets_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<BulkIdsReq>,
) -> Result<Json<Vec<BulkIdResponse>>, ErrorResp> {
    Ok(Json(
        state.services.memory.remove_assets(&auth, &id, &dto).await?,
    ))
}
