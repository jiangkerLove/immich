use axum::extract::State;
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::download::{DownloadArchiveReq, DownloadInfoReq, DownloadResponse};

pub async fn get_download_info_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<DownloadInfoReq>,
) -> Result<Json<DownloadResponse>, ErrorResp> {
    Ok(Json(
        state.services.download.get_download_info(&auth, &dto).await?,
    ))
}

pub async fn download_archive_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<DownloadArchiveReq>,
) -> Result<axum::response::Response, ErrorResp> {
    state.services.download.download_archive(&auth, &dto).await
}
