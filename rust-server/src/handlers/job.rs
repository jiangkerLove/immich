use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::queue::{ManualJobCreateReq, QueueCommandReq, QueueLegacyResponse, QueuesLegacyResponse};

pub async fn get_jobs_legacy_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<QueuesLegacyResponse>, ErrorResp> {
    Ok(Json(state.services.queue.get_all_legacy(&auth).await?))
}

pub async fn create_job_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<ManualJobCreateReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.queue.create_manual_job(&auth, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn run_queue_command_legacy_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(name): Path<String>,
    Json(dto): Json<QueueCommandReq>,
) -> Result<Json<QueueLegacyResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .queue
            .run_legacy_command(&auth, &name, &dto)
            .await?,
    ))
}
