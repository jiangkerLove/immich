use crate::app_state::AppState;
use crate::dtos::auth_dto::AuthDto;
use crate::dtos::response_dto::ErrorDto;
use crate::dtos::user_dto::UserAdminResponseDto;
use axum::extract::State;
use axum::routing::get;
use axum::{Extension, Router};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/users/me", get(get_my_user))
}

async fn get_my_user(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<UserAdminResponseDto, ErrorDto> {
    state.auth_service.get_me(&state.sql_pool, &auth).await
}