use crate::app_state::AppState;
use crate::dtos::auth_dto::{LoginCredentialDto, LoginDetails, LoginResponseDto};
use crate::dtos::response_dto::ErrorDto;
use axum::extract::State;
use axum::{Extension, Json};

pub async fn login_handler(
    State(state): State<AppState>,
    Extension(login_details): Extension<LoginDetails>,
    Json(login_credential): Json<LoginCredentialDto>,
) -> Result<LoginResponseDto, ErrorDto> {
    state.auth_service.login(&login_credential, &login_details).await
}
