use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::asset_file::{AssetFileResponse, AssetFileSearchQuery};

pub async fn search_asset_files_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<AssetFileSearchQuery>,
) -> Result<Json<Vec<AssetFileResponse>>, ErrorResp> {
    Ok(Json(
        state.services.asset_file.search(&auth, &query).await?,
    ))
}

pub async fn get_asset_file_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<AssetFileResponse>, ErrorResp> {
    Ok(Json(state.services.asset_file.get(&auth, &id).await?))
}

pub async fn download_asset_file_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, ErrorResp> {
    state.services.asset_file.download(&auth, &id).await
}

pub async fn delete_asset_file_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.asset_file.delete(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
