use crate::app_state::AppState;
use crate::dtos::auth::{LoginCredentialDto, LoginDetails, LoginResponseDto};
use crate::dtos::response::ErrorDto;
use axum::extract::State;
use axum::{Extension, Json};

pub async fn login(
    State(state): State<AppState>,
    Extension(login_details): Extension<LoginDetails>,
    Json(login_credential): Json<LoginCredentialDto>,
) -> Result<LoginResponseDto, ErrorDto> {
    state.auth_service.login(&state.sql_pool, &login_credential, &login_details).await
}
