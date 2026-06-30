use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Response, StatusCode};
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::dto::maintenance::{
    MaintenanceDetectInstallResp, MaintenanceLoginReq, MaintenanceStatusResp, SetMaintenanceModeReq,
};
use crate::models::request::auth::LoginReq;
use crate::models::response::response::ErrorResp;
use crate::utils::cookie::{parse_immich_cookies, ImmichCookie};
use crate::utils::response::respond_with_maintenance_cookie;

fn maintenance_token(headers: &HeaderMap) -> Option<String> {
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())?;
    parse_immich_cookies(cookie)
        .get(&ImmichCookie::MaintenanceToken)
        .cloned()
}

pub async fn maintenance_status_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MaintenanceStatusResp>, ErrorResp> {
    let token = maintenance_token(&headers);
    if let Some(worker) = state.maintenance_worker.as_ref() {
        return Ok(Json(worker.status(token.as_deref()).await));
    }

    Ok(Json(
        state.services.maintenance.get_maintenance_status().await?,
    ))
}

pub async fn maintenance_login_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut dto): Json<MaintenanceLoginReq>,
) -> Result<Json<crate::models::dto::maintenance::MaintenanceAuthResp>, ErrorResp> {
    if dto.token.is_none() {
        dto.token = maintenance_token(&headers);
    }

    if let Some(worker) = state.maintenance_worker.as_ref() {
        return Ok(Json(worker.maintenance_login(&dto).await?));
    }

    Ok(Json(
        state.services.maintenance.maintenance_login(&dto).await?,
    ))
}

pub async fn detect_prior_install_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<MaintenanceDetectInstallResp>, ErrorResp> {
    Ok(Json(
        state.services.maintenance.detect_prior_install(&auth).await?,
    ))
}

pub async fn maintenance_detect_prior_install_handler(
    State(state): State<AppState>,
) -> Result<Json<MaintenanceDetectInstallResp>, ErrorResp> {
    let worker = state
        .maintenance_worker
        .as_ref()
        .ok_or_else(|| ErrorResp::ServerError("Maintenance worker unavailable".to_string()))?;
    Ok(Json(worker.detect_prior_install().await?))
}

pub async fn set_maintenance_mode_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Extension(login_details): Extension<LoginReq>,
    Json(dto): Json<SetMaintenanceModeReq>,
) -> Result<Response<Body>, ErrorResp> {
    let jwt = state
        .services
        .maintenance
        .set_maintenance_mode(&auth, &dto)
        .await?;

    if jwt.is_empty() {
        return Ok(Response::builder()
            .status(axum::http::StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap());
    }

    Ok(respond_with_maintenance_cookie(
        login_details.is_secure,
        &jwt,
    ))
}

pub async fn maintenance_set_action_handler(
    State(state): State<AppState>,
    Json(dto): Json<SetMaintenanceModeReq>,
) -> Result<StatusCode, ErrorResp> {
    let worker = state
        .maintenance_worker
        .as_ref()
        .ok_or_else(|| ErrorResp::ServerError("Maintenance worker unavailable".to_string()))?;
    worker.set_action(dto).await;
    Ok(StatusCode::NO_CONTENT)
}
