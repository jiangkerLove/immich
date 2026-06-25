use axum::extract::State;
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::trash::{BulkIdsReq, TrashResponse};

pub async fn empty_trash_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<TrashResponse>, ErrorResp> {
    Ok(Json(state.services.trash.empty(&auth).await?))
}

pub async fn restore_trash_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<TrashResponse>, ErrorResp> {
    Ok(Json(state.services.trash.restore(&auth).await?))
}

pub async fn restore_assets_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<BulkIdsReq>,
) -> Result<Json<TrashResponse>, ErrorResp> {
    Ok(Json(
        state.services.trash.restore_assets(&auth, &dto).await?,
    ))
}
