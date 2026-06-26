use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::queue::{
    QueueDeleteReq, QueueJobResponse, QueueJobSearchQuery, QueueResponse, QueueUpdateReq,
};

pub async fn get_queues_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<QueueResponse>>, ErrorResp> {
    Ok(Json(state.services.queue.get_all(&auth).await?))
}

pub async fn get_queue_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(name): Path<String>,
) -> Result<Json<QueueResponse>, ErrorResp> {
    Ok(Json(state.services.queue.get(&auth, &name).await?))
}

pub async fn update_queue_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(name): Path<String>,
    Json(dto): Json<QueueUpdateReq>,
) -> Result<Json<QueueResponse>, ErrorResp> {
    Ok(Json(
        state.services.queue.update(&auth, &name, &dto).await?,
    ))
}

pub async fn get_queue_jobs_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(name): Path<String>,
    Query(query): Query<QueueJobSearchQuery>,
) -> Result<Json<Vec<QueueJobResponse>>, ErrorResp> {
    Ok(Json(
        state
            .services
            .queue
            .search_jobs(&auth, &name, &query)
            .await?,
    ))
}

pub async fn empty_queue_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(name): Path<String>,
    Json(dto): Json<QueueDeleteReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.queue.empty_queue(&auth, &name, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}
