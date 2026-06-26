use axum::extract::State;
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::db::system_metadata::{AdminOnboarding, ReverseGeocodingState, VersionCheckState};
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;

pub async fn get_admin_onboarding_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<AdminOnboarding>, ErrorResp> {
    Ok(Json(
        state
            .services
            .system_metadata
            .get_admin_onboarding(&auth)
            .await?,
    ))
}

pub async fn update_admin_onboarding_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<AdminOnboarding>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .system_metadata
        .update_admin_onboarding(&auth, &dto)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_reverse_geocoding_state_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<ReverseGeocodingState>, ErrorResp> {
    Ok(Json(
        state
            .services
            .system_metadata
            .get_reverse_geocoding_state(&auth)
            .await?,
    ))
}

pub async fn get_version_check_state_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<VersionCheckState>, ErrorResp> {
    Ok(Json(
        state
            .services
            .system_metadata
            .get_version_check_state(&auth)
            .await?,
    ))
}
