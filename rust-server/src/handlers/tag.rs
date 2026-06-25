use axum::extract::{Path, State};
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::tag::{TagCreateReq, TagResponse, TagUpdateReq};

pub async fn get_tags_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<TagResponse>>, ErrorResp> {
    Ok(Json(state.services.tag.get_all(&auth).await?))
}

pub async fn get_tag_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<TagResponse>, ErrorResp> {
    Ok(Json(state.services.tag.get(&auth, &id).await?))
}

pub async fn create_tag_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<TagCreateReq>,
) -> Result<Json<TagResponse>, ErrorResp> {
    Ok(Json(state.services.tag.create(&auth, &dto).await?))
}

pub async fn update_tag_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<TagUpdateReq>,
) -> Result<Json<TagResponse>, ErrorResp> {
    Ok(Json(state.services.tag.update(&auth, &id, &dto).await?))
}

pub async fn delete_tag_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<(), ErrorResp> {
    state.services.tag.delete(&auth, &id).await
}
