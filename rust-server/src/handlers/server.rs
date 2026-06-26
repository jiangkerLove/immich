use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Response, StatusCode};
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::server::{
    ServerAboutResponse, ServerConfigResponse, ServerFeaturesResponse, ServerMediaTypesResponse,
    ServerPingResponse, ServerService, ServerStatsResponse, ServerStorageResponse,
    ServerVersionHistoryResponse, ServerVersionResponse, WellKnownApi, WellKnownResponse,
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

pub async fn statistics_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<ServerStatsResponse>, ErrorResp> {
    Ok(Json(state.services.server.get_statistics(&auth).await?))
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

pub async fn apk_links_handler(
    State(state): State<AppState>,
) -> Json<crate::service::server::ServerApkLinksResponse> {
    Json(state.services.server.get_apk_links())
}

pub async fn get_server_license_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<crate::models::response::user::UserLicenseResponse>, ErrorResp> {
    Ok(Json(state.services.server.get_license(&auth).await?))
}

pub async fn set_server_license_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<crate::service::server::LicenseKeyReq>,
) -> Result<Json<crate::models::response::user::UserLicenseResponse>, ErrorResp> {
    Ok(Json(state.services.server.set_license(&auth, &dto).await?))
}

pub async fn delete_server_license_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<StatusCode, ErrorResp> {
    state.services.server.delete_license(&auth).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn version_check_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<crate::models::db::system_metadata::VersionCheckState>, ErrorResp> {
    Ok(Json(state.services.server.get_version_check(&auth).await?))
}
