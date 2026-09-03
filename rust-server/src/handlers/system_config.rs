use axum::extract::State;
use axum::Extension;
use axum::Json;
use serde_json::Value;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;

pub async fn get_system_config_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Value>, ErrorResp> {
    Ok(Json(state.services.system_config.get_config(&auth).await?))
}

pub async fn get_system_config_defaults_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Value>, ErrorResp> {
    Ok(Json(state.services.system_config.get_defaults(&auth)?))
}

pub async fn update_system_config_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<Value>,
) -> Result<Json<Value>, ErrorResp> {
    Ok(Json(
        state.services.system_config.update_config(&auth, &dto).await?,
    ))
}

pub async fn get_admin_config_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Value>, ErrorResp> {
    Ok(Json(
        state.services.system_config.get_admin_config(&auth).await?,
    ))
}

pub async fn get_admin_config_defaults_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Value>, ErrorResp> {
    Ok(Json(
        state.services.system_config.get_admin_config_defaults(&auth)?,
    ))
}

pub async fn update_admin_config_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<Value>,
) -> Result<Json<Value>, ErrorResp> {
    Ok(Json(
        state
            .services
            .system_config
            .update_admin_config(&auth, &dto)
            .await?,
    ))
}

pub async fn get_storage_template_options_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Value>, ErrorResp> {
    Ok(Json(
        state
            .services
            .system_config
            .storage_template_options(&auth)?,
    ))
}
