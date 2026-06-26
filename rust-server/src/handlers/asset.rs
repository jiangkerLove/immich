use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::{AssetResponse, AssetStatsResponse};
use crate::models::response::response::ErrorResp;
use crate::service::asset::{
    AssetBulkDeleteReq, AssetBulkUpdateReq, AssetCopyReq, AssetEditsCreateReq, AssetEditsResponse,
    AssetMetadataBulkDeleteReq, AssetMetadataBulkResponse, AssetMetadataBulkUpsertReq,
    AssetMetadataResponse, AssetMetadataUpsertReq, AssetStatsQuery, AssetJobsReq, UpdateAssetReq,
};

pub async fn get_asset_statistics_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<AssetStatsQuery>,
) -> Result<Json<AssetStatsResponse>, ErrorResp> {
    Ok(Json(
        state.services.asset.get_statistics(&auth, &query).await?,
    ))
}

pub async fn copy_asset_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<AssetCopyReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.asset.copy(&auth, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn run_asset_jobs_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<AssetJobsReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.asset.run_jobs(&auth, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
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

pub async fn get_asset_metadata_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<AssetMetadataResponse>>, ErrorResp> {
    Ok(Json(state.services.asset.get_metadata(&auth, &id).await?))
}

pub async fn upsert_asset_metadata_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<AssetMetadataUpsertReq>,
) -> Result<Json<Vec<AssetMetadataResponse>>, ErrorResp> {
    Ok(Json(
        state.services.asset.upsert_metadata(&auth, &id, &dto).await?,
    ))
}

pub async fn get_asset_metadata_by_key_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<AssetMetadataResponse>, ErrorResp> {
    Ok(Json(
        state.services.asset.get_metadata_by_key(&auth, &id, &key).await?,
    ))
}

pub async fn delete_asset_metadata_by_key_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .asset
        .delete_metadata_by_key(&auth, &id, &key)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn upsert_bulk_asset_metadata_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<AssetMetadataBulkUpsertReq>,
) -> Result<Json<Vec<AssetMetadataBulkResponse>>, ErrorResp> {
    Ok(Json(
        state
            .services
            .asset
            .upsert_bulk_metadata(&auth, &dto)
            .await?,
    ))
}

pub async fn delete_bulk_asset_metadata_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<AssetMetadataBulkDeleteReq>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .asset
        .delete_bulk_metadata(&auth, &dto)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_asset_edits_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<AssetEditsResponse>, ErrorResp> {
    Ok(Json(state.services.asset.get_edits(&auth, &id).await?))
}

pub async fn replace_asset_edits_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<AssetEditsCreateReq>,
) -> Result<Json<AssetEditsResponse>, ErrorResp> {
    Ok(Json(
        state.services.asset.replace_edits(&auth, &id, &dto).await?,
    ))
}

pub async fn delete_asset_edits_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.asset.delete_edits(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_asset_ocr_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<crate::models::db::asset_ocr::AssetOcrRow>>, ErrorResp> {
    Ok(Json(state.services.asset.get_ocr(&auth, &id).await?))
}
