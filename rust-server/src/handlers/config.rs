use axum::extract::State;
use axum::Extension;
use axum::Json;
use serde_json::Value;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;

pub async fn get_user_config_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Value>, ErrorResp> {
    Ok(Json(
        state.services.system_config.get_user_config(&auth).await?,
    ))
}

pub async fn get_user_config_defaults_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Value>, ErrorResp> {
    Ok(Json(
        state
            .services
            .system_config
            .get_user_config_defaults(&auth)?,
    ))
}

pub async fn get_public_config_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, ErrorResp> {
    Ok(Json(
        state.services.system_config.get_public_config().await?,
    ))
}

pub async fn get_public_config_defaults_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, ErrorResp> {
    Ok(Json(
        state
            .services
            .system_config
            .get_public_config_defaults(),
    ))
}
