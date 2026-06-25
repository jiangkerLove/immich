use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Response, StatusCode};
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::request::auth::LoginReq;
use crate::models::response::response::ErrorResp;
use crate::models::response::shared_link::SharedLinkResponse;
use crate::service::shared_link::{
    merge_shared_link_tokens, AssetIdsReq, AssetIdsResponse, SharedLinkCreateReq, SharedLinkEditReq,
    SharedLinkLoginReq, SharedLinkSearchQuery,
};
use crate::utils::headers::get_shared_link_tokens;
use crate::utils::response::respond_with_shared_link_cookie;

pub async fn get_shared_links_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<SharedLinkSearchQuery>,
) -> Result<Json<Vec<SharedLinkResponse>>, ErrorResp> {
    Ok(Json(
        state
            .services
            .shared_link
            .get_all(&auth, &query)
            .await?,
    ))
}

pub async fn get_shared_link_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<SharedLinkResponse>, ErrorResp> {
    Ok(Json(state.services.shared_link.get(&auth, &id).await?))
}

pub async fn create_shared_link_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<SharedLinkCreateReq>,
) -> Result<Json<SharedLinkResponse>, ErrorResp> {
    Ok(Json(state.services.shared_link.create(&auth, &dto).await?))
}

pub async fn update_shared_link_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<SharedLinkEditReq>,
) -> Result<Json<SharedLinkResponse>, ErrorResp> {
    Ok(Json(
        state.services.shared_link.update(&auth, &id, &dto).await?,
    ))
}

pub async fn delete_shared_link_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.shared_link.remove(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn shared_link_login_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Extension(login_details): Extension<LoginReq>,
    headers: HeaderMap,
    Json(dto): Json<SharedLinkLoginReq>,
) -> Result<Response<Body>, ErrorResp> {
    let (body, token) = state.services.shared_link.login(&auth, &dto).await?;
    let merged = merge_shared_link_tokens(&get_shared_link_tokens(&headers), &token);
    Ok(respond_with_shared_link_cookie(
        &body,
        login_details.is_secure,
        &merged,
    ))
}

pub async fn get_my_shared_link_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    headers: HeaderMap,
) -> Result<Json<SharedLinkResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .shared_link
            .get_mine(&auth, &get_shared_link_tokens(&headers))
            .await?,
    ))
}

pub async fn add_shared_link_assets_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<AssetIdsReq>,
) -> Result<Json<Vec<AssetIdsResponse>>, ErrorResp> {
    Ok(Json(
        state
            .services
            .shared_link
            .add_assets(&auth, &id, &dto)
            .await?,
    ))
}

pub async fn remove_shared_link_assets_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<AssetIdsReq>,
) -> Result<Json<Vec<AssetIdsResponse>>, ErrorResp> {
    Ok(Json(
        state
            .services
            .shared_link
            .remove_assets(&auth, &id, &dto)
            .await?,
    ))
}
