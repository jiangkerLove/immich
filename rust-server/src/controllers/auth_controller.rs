use crate::app_state::AppState;
use crate::dtos::auth_dto::{LoginCredentialDto, LoginDetails, LoginResponseDto};
use crate::dtos::response_dto::ErrorDto;
use axum::extract::State;
use axum::routing::post;
use axum::{Extension, Json, Router};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/login", post(login))
}

async fn login(
    State(state): State<AppState>,
    Extension(login_details): Extension<LoginDetails>,
    Json(login_credential): Json<LoginCredentialDto>,
) -> Result<LoginResponseDto, ErrorDto> {
    state.auth_service.login(&state.sql_pool, &login_credential, &login_details).await
}
