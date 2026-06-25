use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Response};
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::server::{
    ServerAboutResponse, ServerConfigResponse, ServerFeaturesResponse, ServerMediaTypesResponse,
    ServerPingResponse, ServerService, ServerStorageResponse, ServerVersionHistoryResponse,
    ServerVersionResponse, WellKnownApi, WellKnownResponse,
};

pub async fn ping_handler() -> ServerPingResponse {
    ServerService::ping()
}

pub async fn version_handler() -> ServerVersionResponse {
    ServerService::version()
}

pub async fn version_history_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<ServerVersionHistoryResponse>>, ErrorResp> {
    Ok(Json(state.services.server.get_version_history().await?))
}

pub async fn features_handler(
    State(state): State<AppState>,
) -> Result<ServerFeaturesResponse, ErrorResp> {
    state.services.server.get_features().await
}

pub async fn config_handler(
    State(state): State<AppState>,
) -> Result<ServerConfigResponse, ErrorResp> {
    state.services.server.get_config().await
}

pub async fn about_handler(
    State(state): State<AppState>,
) -> Result<ServerAboutResponse, ErrorResp> {
    state.services.server.get_about().await
}

pub async fn storage_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<ServerStorageResponse, ErrorResp> {
    state.services.server.get_storage(&auth)
}

pub async fn media_types_handler() -> ServerMediaTypesResponse {
    ServerService::get_media_types()
}

pub async fn well_known_handler() -> WellKnownResponse {
    WellKnownResponse {
        api: WellKnownApi {
            endpoint: "/api".to_string(),
        },
    }
}

pub async fn custom_css_handler(
    State(state): State<AppState>,
) -> Result<Response<Body>, ErrorResp> {
    let css = state.services.server.get_custom_css().await?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
        .body(Body::from(css))
        .unwrap())
}
