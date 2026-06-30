use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{Response, StatusCode};
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::request::auth::LoginReq;
use crate::models::response::response::ErrorResp;
use crate::service::database_backup::{DatabaseBackupDeleteReq, DatabaseBackupListResponse};

pub async fn list_database_backups_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<DatabaseBackupListResponse>, ErrorResp> {
    Ok(Json(
        state.services.database_backup.list_backups(&auth).await?,
    ))
}

pub async fn download_database_backup_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(filename): Path<String>,
) -> Result<Response<Body>, ErrorResp> {
    state
        .services
        .database_backup
        .download_backup(&auth, &filename)
        .await
}

pub async fn delete_database_backups_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<DatabaseBackupDeleteReq>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .database_backup
        .delete_backups(&auth, &dto.backups)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn upload_database_backup_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    mut multipart: Multipart,
) -> Result<StatusCode, ErrorResp> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_name = String::from("backup.sql.gz");

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ErrorResp::BadRequest(err.to_string()))?
    {
        if field.name().unwrap_or("") == "file" {
            original_name = field
                .file_name()
                .unwrap_or("backup.sql.gz")
                .to_string();
            file_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|err| ErrorResp::BadRequest(err.to_string()))?
                    .to_vec(),
            );
        }
    }

    let file_bytes =
        file_bytes.ok_or_else(|| ErrorResp::BadRequest("file is required".to_string()))?;

    state
        .services
        .database_backup
        .upload_backup(&auth, &original_name, file_bytes)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn start_database_restore_handler(
    State(state): State<AppState>,
    Extension(login_details): Extension<LoginReq>,
) -> Result<Response<Body>, ErrorResp> {
    state
        .services
        .maintenance
        .start_restore_flow(login_details.is_secure)
        .await
}

pub async fn maintenance_list_database_backups_handler(
    State(state): State<AppState>,
) -> Result<Json<DatabaseBackupListResponse>, ErrorResp> {
    Ok(Json(
        state.services.database_backup.list_backups_internal().await?,
    ))
}

pub async fn maintenance_download_database_backup_handler(
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<Response<Body>, ErrorResp> {
    state
        .services
        .database_backup
        .download_backup_internal(&filename)
        .await
}

pub async fn maintenance_delete_database_backups_handler(
    State(state): State<AppState>,
    Json(dto): Json<DatabaseBackupDeleteReq>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .database_backup
        .delete_backups_internal(&dto.backups)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn maintenance_upload_database_backup_handler(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<StatusCode, ErrorResp> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_name = String::from("backup.sql.gz");

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ErrorResp::BadRequest(err.to_string()))?
    {
        if field.name().unwrap_or("") == "file" {
            original_name = field
                .file_name()
                .unwrap_or("backup.sql.gz")
                .to_string();
            file_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|err| ErrorResp::BadRequest(err.to_string()))?
                    .to_vec(),
            );
        }
    }

    let file_bytes =
        file_bytes.ok_or_else(|| ErrorResp::BadRequest("file is required".to_string()))?;

    state
        .services
        .database_backup
        .upload_backup_internal(&original_name, file_bytes)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
