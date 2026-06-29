use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::Extension;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::hls::HlsService;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlsPositionQuery {
    #[serde(default)]
    pub position: Option<f64>,
}

pub async fn get_main_playlist_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Response, ErrorResp> {
    let body = state.services.hls.get_main_playlist(&auth, id).await?;
    Ok((
        [
            ("Cache-Control", "no-cache"),
            ("Content-Type", HlsService::playlist_content_type()),
        ],
        body,
    )
        .into_response())
}

pub async fn get_media_playlist_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path((id, session_id, variant_index)): Path<(Uuid, Uuid, u32)>,
    Query(query): Query<HlsPositionQuery>,
) -> Result<Response, ErrorResp> {
    let body = state
        .services
        .hls
        .get_media_playlist(&auth, id, session_id, variant_index, query.position)
        .await?;
    Ok((
        [
            ("Cache-Control", "no-cache"),
            ("Content-Type", HlsService::playlist_content_type()),
        ],
        body,
    )
        .into_response())
}

pub async fn get_segment_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path((id, session_id, variant_index, filename)): Path<(Uuid, Uuid, u32, String)>,
    headers: HeaderMap,
) -> Result<Response, ErrorResp> {
    let init_segment = headers
        .get("immich-hls-init-segment")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());

    let path = state
        .services
        .hls
        .get_segment_path(
            &auth,
            id,
            session_id,
            variant_index,
            &filename,
            init_segment,
        )
        .await?;

    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|err| ErrorResp::NotFound(format!("Segment not found: {err}")))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok((
        [
            (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600")),
            (header::CONTENT_TYPE, HeaderValue::from_static(HlsService::segment_content_type())),
        ],
        body,
    )
        .into_response())
}

pub async fn end_session_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path((id, session_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .hls
        .end_session(&auth, id, session_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
