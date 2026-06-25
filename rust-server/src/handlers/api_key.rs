use axum::extract::{Path, State};
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::api_key::{
    ApiKeyCreateReq, ApiKeyCreateResp, ApiKeyResponse, ApiKeyUpdateReq,
};

pub async fn get_api_keys_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<ApiKeyResponse>>, ErrorResp> {
    Ok(Json(state.services.api_key.get_all(&auth).await?))
}

pub async fn get_api_key_me_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<ApiKeyResponse>, ErrorResp> {
    Ok(Json(state.services.api_key.get_me(&auth).await?))
}

pub async fn get_api_key_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiKeyResponse>, ErrorResp> {
    Ok(Json(state.services.api_key.get(&auth, &id).await?))
}

pub async fn create_api_key_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<ApiKeyCreateReq>,
) -> Result<Json<ApiKeyCreateResp>, ErrorResp> {
    Ok(Json(state.services.api_key.create(&auth, &dto).await?))
}

pub async fn update_api_key_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<ApiKeyUpdateReq>,
) -> Result<Json<ApiKeyResponse>, ErrorResp> {
    Ok(Json(
        state.services.api_key.update(&auth, &id, &dto).await?,
    ))
}

pub async fn delete_api_key_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<(), ErrorResp> {
    state.services.api_key.delete(&auth, &id).await
}
