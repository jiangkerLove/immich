use axum::extract::{Path, Query, State};
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::album::{
    AlbumResponse, AlbumStatisticsResponse, CreateAlbumReq, GetAlbumsQuery,
};

pub async fn get_album_statistics_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<AlbumStatisticsResponse>, ErrorResp> {
    Ok(Json(state.services.album.get_statistics(&auth).await?))
}

pub async fn get_albums_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<GetAlbumsQuery>,
) -> Result<Json<Vec<AlbumResponse>>, ErrorResp> {
    Ok(Json(state.services.album.get_all(&auth, &query).await?))
}

pub async fn create_album_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<CreateAlbumReq>,
) -> Result<Json<AlbumResponse>, ErrorResp> {
    Ok(Json(state.services.album.create(&auth, &dto).await?))
}

pub async fn get_album_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<AlbumResponse>, ErrorResp> {
    Ok(Json(state.services.album.get(&auth, &id).await?))
}
