use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::library::{
    CreateLibraryReq, LibraryResponse, LibraryStatsResponse, UpdateLibraryReq, ValidateLibraryReq,
    ValidateLibraryResponse,
};

pub async fn get_libraries_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<LibraryResponse>>, ErrorResp> {
    Ok(Json(state.services.library.get_all(&auth).await?))
}

pub async fn create_library_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<CreateLibraryReq>,
) -> Result<Json<LibraryResponse>, ErrorResp> {
    Ok(Json(state.services.library.create(&auth, &dto).await?))
}

pub async fn get_library_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<LibraryResponse>, ErrorResp> {
    Ok(Json(state.services.library.get(&auth, &id).await?))
}

pub async fn update_library_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateLibraryReq>,
) -> Result<Json<LibraryResponse>, ErrorResp> {
    Ok(Json(state.services.library.update(&auth, &id, &dto).await?))
}

pub async fn patch_library_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateLibraryReq>,
) -> Result<Json<LibraryResponse>, ErrorResp> {
    Ok(Json(state.services.library.update(&auth, &id, &dto).await?))
}

pub async fn delete_library_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.library.delete(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn validate_library_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(_id): Path<Uuid>,
    Json(dto): Json<ValidateLibraryReq>,
) -> Result<Json<ValidateLibraryResponse>, ErrorResp> {
    Ok(Json(state.services.library.validate(&auth, &dto).await?))
}

pub async fn get_library_statistics_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<LibraryStatsResponse>, ErrorResp> {
    Ok(Json(
        state.services.library.get_statistics(&auth, &id).await?,
    ))
}

pub async fn scan_library_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.library.queue_scan(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
