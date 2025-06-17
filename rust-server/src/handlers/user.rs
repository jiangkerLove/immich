use crate::app_state::AppState;
use crate::dtos::auth_dto::AuthDto;
use crate::dtos::response_dto::ErrorDto;
use crate::dtos::user_dto::UserAdminResponseDto;
use crate::dtos::user_preferences_response_dto::UserPreferenceResponseDto;
use axum::extract::State;
use axum::Extension;

pub async fn get_my_user_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<UserAdminResponseDto, ErrorDto> {
    state.auth_service.get_me(&auth).await
}

pub async fn get_my_preferences_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<UserPreferenceResponseDto, ErrorDto> {
    state.auth_service.get_me_preferences(&auth).await
}