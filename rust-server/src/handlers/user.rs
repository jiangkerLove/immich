use crate::app_state::AppState;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::models::response::user_preferences_response::UserPreferenceResponse;
use axum::extract::State;
use axum::Extension;
use crate::models::dto::auth::AuthDto;

pub async fn get_my_user_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.auth_service.get_me(&auth).await
}

pub async fn get_my_preferences_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<UserPreferenceResponse, ErrorResp> {
    state.auth_service.get_me_preferences(&auth).await
}