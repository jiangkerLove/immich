use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Response, StatusCode};
use axum::Extension;
use axum::Json;
use futures_util::stream;

use crate::app_state::AppState;
use crate::models::db::sync_checkpoint::SyncCheckpointRow;
use crate::models::dto::auth::AuthDto;
use crate::models::request::sync::{SyncAckDeleteReq, SyncAckSetReq, SyncStreamReq};
use crate::models::response::response::ErrorResp;

pub async fn stream_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(req): Json<SyncStreamReq>,
) -> Result<Response<Body>, ErrorResp> {
    let lines = state.services.sync.stream(&auth, &req).await?;
    let body = Body::from_stream(stream::iter(
        lines.into_iter().map(Ok::<_, std::convert::Infallible>),
    ));

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/jsonlines+json")
        .body(body)
        .map_err(|e| ErrorResp::ServerError(e.to_string()))
}

pub async fn get_ack_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<SyncCheckpointRow>>, ErrorResp> {
    Ok(Json(state.services.sync.get_acks(&auth).await?))
}

pub async fn set_ack_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(req): Json<SyncAckSetReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.sync.set_acks(&auth, &req).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_ack_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(req): Json<SyncAckDeleteReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.sync.delete_acks(&auth, &req).await?;
    Ok(StatusCode::NO_CONTENT)
}
