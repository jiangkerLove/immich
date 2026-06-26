use axum::body::Body;
use axum::extract::State;
use axum::http::Response;
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::dto::maintenance::{
    MaintenanceDetectInstallResp, MaintenanceLoginReq, MaintenanceStatusResp, SetMaintenanceModeReq,
};
use crate::models::request::auth::LoginReq;
use crate::models::response::response::ErrorResp;
use crate::utils::response::respond_with_maintenance_cookie;

pub async fn maintenance_status_handler(
    State(state): State<AppState>,
) -> Result<Json<MaintenanceStatusResp>, ErrorResp> {
    Ok(Json(
        state.services.maintenance.get_maintenance_status().await?,
    ))
}

pub async fn maintenance_login_handler(
    State(state): State<AppState>,
    Json(dto): Json<MaintenanceLoginReq>,
) -> Result<Json<crate::models::dto::maintenance::MaintenanceAuthResp>, ErrorResp> {
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
