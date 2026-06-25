use axum::extract::{Multipart, Path, Query, State};
use axum::Extension;
use axum::Json;
use axum::http::StatusCode;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::asset_media::{
    AssetMediaCreateReq, AssetMediaOptionsQuery, AssetMediaResponse, BulkUploadCheckReq,
    BulkUploadCheckResponse,
};

pub async fn upload_asset_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<AssetMediaResponse>), ErrorResp> {
    let mut dto: Option<AssetMediaCreateReq> = None;
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_name = String::from("upload.bin");

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ErrorResp::BadRequest(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "assetData" {
            original_name = field.file_name().unwrap_or("upload.bin").to_string();
            file_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| ErrorResp::BadRequest(e.to_string()))?
                    .to_vec(),
            );
        } else if matches!(
            name.as_str(),
            "fileCreatedAt" | "fileModifiedAt" | "filename" | "isFavorite" | "duration" | "visibility" | "livePhotoVideoId"
        ) {
            let text = field
                .text()
                .await
                .map_err(|e| ErrorResp::BadRequest(e.to_string()))?;
            let mut partial = dto.take().unwrap_or(AssetMediaCreateReq {
                file_created_at: chrono::Utc::now(),
                file_modified_at: chrono::Utc::now(),
                filename: None,
                is_favorite: None,
                duration: None,
                live_photo_video_id: None,
                visibility: None,
            });
            match name.as_str() {
                "fileCreatedAt" => {
                    partial.file_created_at = text.parse().unwrap_or(chrono::Utc::now());
                }
                "fileModifiedAt" => {
                    partial.file_modified_at = text.parse().unwrap_or(chrono::Utc::now());
                }
                "filename" => partial.filename = Some(text),
                "isFavorite" => partial.is_favorite = Some(text == "true"),
                "duration" => partial.duration = text.parse().ok(),
                "visibility" => partial.visibility = Some(text),
                "livePhotoVideoId" => {
                    partial.live_photo_video_id = Uuid::parse_str(&text).ok();
                }
                _ => {}
            }
            dto = Some(partial);
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ErrorResp::BadRequest("assetData is required".to_string()))?;
    let dto = dto.unwrap_or(AssetMediaCreateReq {
        file_created_at: chrono::Utc::now(),
        file_modified_at: chrono::Utc::now(),
        filename: Some(original_name.clone()),
        is_favorite: None,
        duration: None,
        live_photo_video_id: None,
        visibility: None,
    });

    let response = state
        .services
        .asset_media
        .upload_asset(&auth, &dto, &file_bytes, &original_name)
        .await?;

    let status = if response.status == "duplicate" {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    Ok((status, Json(response)))
}

pub async fn download_original_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Query(query): Query<AssetMediaOptionsQuery>,
) -> Result<axum::response::Response, ErrorResp> {
    state
        .services
        .asset_media
        .download_original(&auth, &id, query.edited.unwrap_or(false))
        .await
}

pub async fn view_thumbnail_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Query(query): Query<AssetMediaOptionsQuery>,
) -> Result<axum::response::Response, ErrorResp> {
    state
        .services
        .asset_media
        .view_thumbnail(&auth, &id, &query)
        .await
}

pub async fn playback_video_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, ErrorResp> {
    state.services.asset_media.playback_video(&auth, &id).await
}

pub async fn bulk_upload_check_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<BulkUploadCheckReq>,
) -> Result<Json<BulkUploadCheckResponse>, ErrorResp> {
    Ok(Json(
        state.services.asset_media.bulk_upload_check(&auth, &dto).await?,
    ))
}
