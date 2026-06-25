use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::{AssetResponse, AssetStatsResponse};
use crate::models::response::response::ErrorResp;
use crate::service::asset::{AssetBulkDeleteReq, AssetBulkUpdateReq, AssetStatsQuery, UpdateAssetReq};

pub async fn get_asset_statistics_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<AssetStatsQuery>,
) -> Result<Json<AssetStatsResponse>, ErrorResp> {
    Ok(Json(
        state.services.asset.get_statistics(&auth, &query).await?,
    ))
}

pub async fn get_asset_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<AssetResponse>, ErrorResp> {
    Ok(Json(state.services.asset.get(&auth, &id).await?))
}

pub async fn update_asset_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateAssetReq>,
) -> Result<Json<AssetResponse>, ErrorResp> {
    Ok(Json(state.services.asset.update(&auth, &id, &dto).await?))
}

pub async fn update_assets_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<AssetBulkUpdateReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.asset.update_all(&auth, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_assets_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<AssetBulkDeleteReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.asset.delete_all(&auth, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}
