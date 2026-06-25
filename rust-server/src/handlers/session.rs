use axum::extract::{Path, State};
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::session::{
    SessionCreateReq, SessionCreateResp, SessionResponse, SessionUpdateReq,
};
use uuid::Uuid;

pub async fn create_session_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<SessionCreateReq>,
) -> Result<Json<SessionCreateResp>, ErrorResp> {
    Ok(Json(
        state.services.session.create(&auth, &dto).await?,
    ))
}

pub async fn get_sessions_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<SessionResponse>>, ErrorResp> {
    Ok(Json(state.services.session.get_all(&auth).await?))
}

pub async fn update_session_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<SessionUpdateReq>,
) -> Result<Json<SessionResponse>, ErrorResp> {
    Ok(Json(
        state.services.session.update(&auth, &id, &dto).await?,
    ))
}

pub async fn delete_session_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<(), ErrorResp> {
    state.services.session.delete(&auth, &id).await
}

pub async fn delete_all_sessions_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<(), ErrorResp> {
    state.services.session.delete_all(&auth).await
}

pub async fn lock_session_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<(), ErrorResp> {
    state.services.session.lock(&auth, &id).await
}
