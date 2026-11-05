use axum::Extension;
use axum::Json;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::asset_media::{
    AssetMediaCreateReq, AssetMediaOptionsQuery, AssetMediaResponse, AssetMetadataItem,
    BulkUploadCheckReq, BulkUploadCheckResponse,
};
use crate::utils::mime_types::{is_asset_path, is_sidecar_path};

pub async fn upload_asset_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<AssetMediaResponse>), ErrorResp> {
    // Match AssetUploadInterceptor: short-circuit before reading the body.
    if let Some(checksum) = headers
        .get("x-immich-checksum")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
    {
        if let Some(id) = state
            .services
            .asset_media
            .get_upload_asset_id_by_checksum(&auth, checksum)
            .await?
        {
            return Ok((
                StatusCode::OK,
                Json(AssetMediaResponse {
                    id,
                    status: "duplicate".to_string(),
                }),
            ));
        }
    }

    let mut file_created_at: Option<DateTime<Utc>> = None;
    let mut file_modified_at: Option<DateTime<Utc>> = None;
    let mut filename: Option<String> = None;
    let mut is_favorite: Option<bool> = None;
    let mut duration: Option<i32> = None;
    let mut live_photo_video_id: Option<Uuid> = None;
    let mut visibility: Option<String> = None;
    let mut metadata: Option<Vec<AssetMetadataItem>> = None;

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_name = String::from("upload.bin");
    let mut sidecar_bytes: Option<Vec<u8>> = None;
    let mut saw_asset = false;
    let mut saw_sidecar = false;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ErrorResp::BadRequest(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "assetData" => {
                if saw_asset {
                    return Err(ErrorResp::BadRequest("Unexpected field".to_string()));
                }
                saw_asset = true;
                let part_name = field.file_name().unwrap_or("upload.bin").to_string();
                let check_name = filename.as_deref().unwrap_or(&part_name);
                if !is_asset_path(check_name) {
                    return Err(ErrorResp::BadRequest(format!(
                        "Unsupported file type {check_name}"
                    )));
                }
                original_name = part_name;
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ErrorResp::BadRequest(e.to_string()))?
                    .to_vec();
                if bytes.is_empty() {
                    return Err(ErrorResp::BadRequest("File is empty".to_string()));
                }
                file_bytes = Some(bytes);
            }
            "sidecarData" => {
                if saw_sidecar {
                    return Err(ErrorResp::BadRequest("Unexpected field".to_string()));
                }
                saw_sidecar = true;
                let part_name = field.file_name().unwrap_or("sidecar.xmp").to_string();
                let check_name = filename.as_deref().unwrap_or(&part_name);
                if !is_sidecar_path(check_name) {
                    return Err(ErrorResp::BadRequest(format!(
                        "Unsupported file type {check_name}"
                    )));
                }
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ErrorResp::BadRequest(e.to_string()))?
                    .to_vec();
                if bytes.is_empty() {
                    return Err(ErrorResp::BadRequest("File is empty".to_string()));
                }
                sidecar_bytes = Some(bytes);
            }
            "fileCreatedAt" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ErrorResp::BadRequest(e.to_string()))?;
                file_created_at = Some(parse_offset_datetime(&text)?);
            }
            "fileModifiedAt" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ErrorResp::BadRequest(e.to_string()))?;
                file_modified_at = Some(parse_offset_datetime(&text)?);
            }
            "filename" => {
                filename = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ErrorResp::BadRequest(e.to_string()))?,
                );
            }
            "isFavorite" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ErrorResp::BadRequest(e.to_string()))?;
                is_favorite = Some(match text.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => {
                        return Err(ErrorResp::BadRequest(
                            "isFavorite must be true or false".to_string(),
                        ));
                    }
                });
            }
            "duration" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ErrorResp::BadRequest(e.to_string()))?;
                duration = Some(parse_duration(&text)?);
            }
            "visibility" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ErrorResp::BadRequest(e.to_string()))?;
                if !matches!(text.as_str(), "archive" | "timeline" | "hidden" | "locked") {
                    return Err(ErrorResp::BadRequest(format!(
                        "Invalid visibility value: {text}"
                    )));
                }
                visibility = Some(text);
            }
            "livePhotoVideoId" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ErrorResp::BadRequest(e.to_string()))?;
                live_photo_video_id = Some(parse_uuid_v4(&text)?);
            }
            "metadata" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ErrorResp::BadRequest(e.to_string()))?;
                metadata = Some(parse_metadata(&text)?);
            }
            _ if field.file_name().is_some() => {
                return Err(ErrorResp::BadRequest("Unexpected field".to_string()));
            }
            _ => {
                // Unknown text fields are ignored (Zod strips them).
                let _ = field.bytes().await;
            }
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| {
        ErrorResp::BadRequest("Field(s) assetData should not be empty".to_string())
    })?;
    let file_created_at = file_created_at
        .ok_or_else(|| ErrorResp::BadRequest("fileCreatedAt is required".to_string()))?;
    let file_modified_at = file_modified_at
        .ok_or_else(|| ErrorResp::BadRequest("fileModifiedAt is required".to_string()))?;

    let dto = AssetMediaCreateReq {
        file_created_at,
        file_modified_at,
        filename,
        is_favorite,
        duration,
        live_photo_video_id,
        visibility,
        metadata,
    };

    let sidecar = sidecar_bytes
        .as_ref()
        .map(|bytes| (bytes.as_slice(), "sidecar.xmp"));
    let response = state
        .services
        .asset_media
        .upload_asset(&auth, &dto, &file_bytes, &original_name, sidecar)
        .await?;

    let status = if response.status == "duplicate" {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    Ok((status, Json(response)))
}

fn parse_offset_datetime(text: &str) -> Result<DateTime<Utc>, ErrorResp> {
    DateTime::parse_from_rfc3339(text)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| {
            // Accept the same ISO-8601-with-offset forms chrono DateTime parses.
            text.parse::<DateTime<Utc>>()
        })
        .map_err(|_| ErrorResp::BadRequest(format!("Invalid datetime: {text}")))
}

fn parse_duration(text: &str) -> Result<i32, ErrorResp> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    let value: f64 = trimmed
        .parse()
        .map_err(|_| ErrorResp::BadRequest(format!("Invalid duration: {text}")))?;
    if !value.is_finite() || value.fract() != 0.0 || value < 0.0 || value > i32::MAX as f64 {
        return Err(ErrorResp::BadRequest(format!("Invalid duration: {text}")));
    }
    Ok(value as i32)
}

fn parse_uuid_v4(text: &str) -> Result<Uuid, ErrorResp> {
    let id = Uuid::parse_str(text)
        .map_err(|_| ErrorResp::BadRequest(format!("Invalid livePhotoVideoId: {text}")))?;
    if id.get_version_num() != 4 {
        return Err(ErrorResp::BadRequest(format!(
            "Invalid livePhotoVideoId: {text}"
        )));
    }
    Ok(id)
}

fn parse_metadata(text: &str) -> Result<Vec<AssetMetadataItem>, ErrorResp> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|_| ErrorResp::BadRequest("Invalid metadata JSON".to_string()))?;
    let items = value
        .as_array()
        .ok_or_else(|| ErrorResp::BadRequest("metadata must be an array".to_string()))?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let obj = item
            .as_object()
            .ok_or_else(|| ErrorResp::BadRequest("metadata items must be objects".to_string()))?;
        let key = obj
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErrorResp::BadRequest("metadata.key must be a string".to_string()))?
            .to_string();
        let value = obj
            .get("value")
            .cloned()
            .ok_or_else(|| ErrorResp::BadRequest("metadata.value is required".to_string()))?;
        if !value.is_object() {
            return Err(ErrorResp::BadRequest(
                "metadata.value must be an object".to_string(),
            ));
        }
        out.push(AssetMetadataItem { key, value });
    }
    Ok(out)
}

pub async fn download_original_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Query(query): Query<AssetMediaOptionsQuery>,
    headers: HeaderMap,
) -> Result<axum::response::Response, ErrorResp> {
    let range = headers
        .get(axum::http::header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    state
        .services
        .asset_media
        .download_original(&auth, &id, query.edited.unwrap_or(false), range)
        .await
}

pub async fn view_thumbnail_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Query(query): Query<AssetMediaOptionsQuery>,
    headers: HeaderMap,
) -> Result<axum::response::Response, ErrorResp> {
    let range = headers
        .get(axum::http::header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    state
        .services
        .asset_media
        .view_thumbnail(&auth, &id, &query, range)
        .await
}

pub async fn playback_video_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<axum::response::Response, ErrorResp> {
    let range = headers
        .get(axum::http::header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    state
        .services
        .asset_media
        .playback_video(&auth, &id, range)
        .await
}

pub async fn bulk_upload_check_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<BulkUploadCheckReq>,
) -> Result<Json<BulkUploadCheckResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .asset_media
            .bulk_upload_check(&auth, &dto)
            .await?,
    ))
}
