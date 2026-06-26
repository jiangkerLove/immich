use axum::extract::{Path, State};
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::duplicate::{DuplicateResolveReq, DuplicateResponse};
use crate::service::album::{BulkIdResponse, BulkIdsReq};

pub async fn get_duplicates_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<DuplicateResponse>>, ErrorResp> {
    Ok(Json(state.services.duplicate.get_all(&auth).await?))
}

pub async fn delete_duplicates_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<BulkIdsReq>,
) -> Result<axum::http::StatusCode, ErrorResp> {
    state.services.duplicate.delete_all(&auth, &dto).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn delete_duplicate_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ErrorResp> {
    state.services.duplicate.delete(&auth, &id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn resolve_duplicates_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<DuplicateResolveReq>,
) -> Result<Json<Vec<BulkIdResponse>>, ErrorResp> {
    Ok(Json(state.services.duplicate.resolve(&auth, &dto).await?))
}
