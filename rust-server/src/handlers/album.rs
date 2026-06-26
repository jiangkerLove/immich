use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::album::{
    AddUsersReq, AlbumResponse, AlbumStatisticsResponse, AlbumsAddAssetsReq, AlbumsAddAssetsResponse,
    BulkIdResponse, BulkIdsReq, CreateAlbumReq, GetAlbumsQuery, UpdateAlbumReq, UpdateAlbumUserReq,
};
use crate::service::map::MapMarkerResponse;

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

pub async fn get_album_map_markers_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<MapMarkerResponse>>, ErrorResp> {
    Ok(Json(
        state.services.map.get_album_map_markers(&auth, &id).await?,
    ))
}

pub async fn update_album_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateAlbumReq>,
) -> Result<Json<AlbumResponse>, ErrorResp> {
    Ok(Json(state.services.album.update(&auth, &id, &dto).await?))
}

pub async fn delete_album_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.album.delete(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn add_assets_to_album_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<BulkIdsReq>,
) -> Result<Json<Vec<BulkIdResponse>>, ErrorResp> {
    Ok(Json(
        state.services.album.add_assets(&auth, &id, &dto).await?,
    ))
}

pub async fn add_assets_to_albums_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<AlbumsAddAssetsReq>,
) -> Result<Json<AlbumsAddAssetsResponse>, ErrorResp> {
    Ok(Json(
        state.services.album.add_assets_to_albums(&auth, &dto).await?,
    ))
}

pub async fn remove_assets_from_album_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<BulkIdsReq>,
) -> Result<Json<Vec<BulkIdResponse>>, ErrorResp> {
    Ok(Json(
        state.services.album.remove_assets(&auth, &id, &dto).await?,
    ))
}

pub async fn add_users_to_album_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<AddUsersReq>,
) -> Result<Json<AlbumResponse>, ErrorResp> {
    Ok(Json(state.services.album.add_users(&auth, &id, &dto).await?))
}

pub async fn update_album_user_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path((id, user_id)): Path<(Uuid, String)>,
    Json(dto): Json<UpdateAlbumUserReq>,
) -> Result<StatusCode, ErrorResp> {
    let user_id = parse_user_id(&auth, &user_id)?;
    state
        .services
        .album
        .update_user(&auth, &id, &user_id, &dto)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_user_from_album_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path((id, user_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, ErrorResp> {
    let user_id = parse_user_id(&auth, &user_id)?;
    state
        .services
        .album
        .remove_user(&auth, &id, &user_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_user_id(auth: &AuthDto, user_id: &str) -> Result<Uuid, ErrorResp> {
    if user_id == "me" {
        Ok(auth.user.id)
    } else {
        Uuid::parse_str(user_id)
            .map_err(|_| ErrorResp::BadRequest("Invalid user ID".to_string()))
    }
}
