use axum::extract::State;
use axum::Extension;

use crate::app_state::AppState;
use crate::models::db::user_metadata::OnboardingPO;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::models::response::user_preferences_response::UserPreferenceResponse;

pub async fn get_my_user_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.user.get_me(&auth).await
}

pub async fn get_my_preferences_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<UserPreferenceResponse, ErrorResp> {
    state.services.user.get_me_preferences(&auth).await
}

pub async fn search_users_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<axum::Json<Vec<UserAdminResponse>>, ErrorResp> {
    Ok(axum::Json(state.services.user.search(&auth).await?))
}

pub async fn get_my_onboarding_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<axum::Json<OnboardingPO>, ErrorResp> {
    Ok(axum::Json(
        state.services.user.get_my_onboarding(&auth).await?,
    ))
}

pub async fn set_my_onboarding_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    axum::Json(dto): axum::Json<OnboardingPO>,
) -> Result<axum::Json<OnboardingPO>, ErrorResp> {
    Ok(axum::Json(
        state.services.user.set_my_onboarding(&auth, &dto).await?,
    ))
}
