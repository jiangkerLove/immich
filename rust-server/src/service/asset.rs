use sqlx::PgPool;
use uuid::Uuid;
use serde::Serialize;
use serde_json::Value;

use crate::service::job::JobService;
use crate::service::websocket::WebSocketHub;
use crate::models::db::assets::{
    self, AssetBasicRow, AssetUpdateFields, ExifUpdateFields, AssetStatsRow,
};
use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::{map_assets, AssetResponse, AssetStatsResponse};
use crate::models::response::response::ErrorResp;
use crate::service::access::require_assets_access;
use crate::utils::permission::require_permission;
use crate::utils::query::parse_query_bool;

#[derive(Clone)]
pub struct AssetService {
    pool: PgPool,
    jobs: JobService,
    websocket: WebSocketHub,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetStatsQuery {
    pub visibility: Option<String>,
    pub is_favorite: Option<String>,
    pub is_trashed: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAssetReq {
    pub is_favorite: Option<bool>,
    pub visibility: Option<String>,
    pub date_time_original: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub rating: Option<i32>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "crate::utils::serde::deserialize_patch_option")]
    pub live_photo_video_id: Option<Option<Uuid>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBulkUpdateReq {
    pub ids: Vec<Uuid>,
    pub is_favorite: Option<bool>,
    pub visibility: Option<String>,
    pub date_time_original: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub rating: Option<i32>,
    pub description: Option<String>,
    pub duplicate_id: Option<Uuid>,
    pub date_time_relative: Option<i32>,
    pub time_zone: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBulkDeleteReq {
    pub ids: Vec<Uuid>,
    pub force: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetCopyReq {
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub shared_links: Option<bool>,
    pub albums: Option<bool>,
    pub sidecar: Option<bool>,
    pub stack: Option<bool>,
    pub favorite: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetJobsReq {
    pub asset_ids: Vec<Uuid>,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMetadataResponse {
    pub key: String,
    pub value: Value,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMetadataBulkResponse {
    pub asset_id: Uuid,
    pub key: String,
    pub value: Value,
    pub updated_at: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMetadataItemReq {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMetadataUpsertReq {
    pub items: Vec<AssetMetadataItemReq>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMetadataBulkItemReq {
    pub asset_id: Uuid,
    pub key: String,
    pub value: Value,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMetadataBulkUpsertReq {
    pub items: Vec<AssetMetadataBulkItemReq>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMetadataBulkDeleteItemReq {
    pub asset_id: Uuid,
    pub key: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMetadataBulkDeleteReq {
    pub items: Vec<AssetMetadataBulkDeleteItemReq>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEditItemReq {
    pub action: String,
    pub parameters: Value,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEditsCreateReq {
    pub edits: Vec<AssetEditItemReq>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEditResponse {
    pub id: Uuid,
    pub action: String,
    pub parameters: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEditsResponse {
    pub asset_id: Uuid,
    pub edits: Vec<AssetEditResponse>,
}

impl AssetService {
    pub fn new(pool: PgPool, jobs: JobService, websocket: WebSocketHub) -> Self {
        Self {
            pool,
            jobs,
            websocket,
        }
    }

    pub async fn get_statistics(
        &self,
        auth: &AuthDto,
        query: &AssetStatsQuery,
    ) -> Result<AssetStatsResponse, ErrorResp> {
        if query.visibility.as_deref() == Some("locked") {
            let elevated = auth
                .session
                .as_ref()
                .is_some_and(|s| s.has_elevated_permission);
            if !elevated {
                return Err(ErrorResp::Forbidden("Forbidden".to_string()));
            }
        }

        require_permission(auth, Permission::AssetRead)?;

        let stats = assets::get_statistics(
            &self.pool,
            &auth.user.id,
            query.visibility.as_deref(),
            query.is_favorite.as_deref().and_then(parse_query_bool),
            query.is_trashed.as_deref().and_then(parse_query_bool).unwrap_or(false),
        )
        .await?;

        Ok(map_stats(&stats))
    }

    pub async fn get(&self, auth: &AuthDto, asset_id: &Uuid) -> Result<AssetResponse, ErrorResp> {
        require_assets_access(&self.pool, auth, &[*asset_id], Permission::AssetRead).await?;

        let row = assets::get_detail_by_id(&self.pool, asset_id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Asset not found".to_string()))?;

        let strip_metadata = auth
            .shared_link
            .as_ref()
            .is_some_and(|link| !link.show_exif);

        let mut responses = map_assets(&self.pool, std::slice::from_ref(&row), auth, strip_metadata)
            .await?;
        responses
            .pop()
            .ok_or_else(|| ErrorResp::BadRequest("Asset not found".to_string()))
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
        dto: &UpdateAssetReq,
    ) -> Result<AssetResponse, ErrorResp> {
        validate_lat_lon(dto.latitude, dto.longitude)?;
        require_assets_access(&self.pool, auth, &[*asset_id], Permission::AssetUpdate).await?;

        let mut previous_motion: Option<AssetBasicRow> = None;
        if let Some(live_photo_video_id) = &dto.live_photo_video_id {
            if let Some(video_id) = live_photo_video_id {
                if let Some(hidden_id) =
                    on_before_link(&self.pool, &auth.user.id, video_id).await?
                {
                    self.websocket
                        .emit_asset_hidden(auth.user.id, hidden_id);
                }
            } else {
                let asset = assets::get_basic_by_id(&self.pool, asset_id)
                    .await?
                    .ok_or_else(|| ErrorResp::BadRequest("Asset not found".to_string()))?;
                if let Some(video_id) = asset.live_photo_video_id {
                    previous_motion = on_before_unlink(&self.pool, &video_id).await?;
                }
            }
        }

        assets::update_asset_fields(
            &self.pool,
            asset_id,
            &AssetUpdateFields {
                is_favorite: dto.is_favorite,
                visibility: dto.visibility.clone(),
                live_photo_video_id: dto.live_photo_video_id,
                duplicate_id: None,
            },
        )
        .await?;

        let exif_fields = build_exif_fields(dto);
        let exif_changed = exif_fields.has_updates();
        assets::update_exif_fields(&self.pool, asset_id, &exif_fields).await?;

        if let Some(motion) = previous_motion {
            let asset = assets::get_basic_by_id(&self.pool, asset_id)
                .await?
                .ok_or_else(|| ErrorResp::BadRequest("Asset not found".to_string()))?;
            on_after_unlink(&self.pool, &auth.user.id, &motion.id, &asset.visibility).await?;
        }

        if exif_changed {
            self.jobs.queue_sidecar_write(asset_id).await?;
        }

        self.get(auth, asset_id).await
    }

    pub async fn update_all(&self, auth: &AuthDto, dto: &AssetBulkUpdateReq) -> Result<(), ErrorResp> {
        validate_lat_lon(dto.latitude, dto.longitude)?;
        require_assets_access(&self.pool, auth, &dto.ids, Permission::AssetUpdate).await?;

        let exif_fields = ExifUpdateFields {
            description: dto.description.clone(),
            date_time_original: dto
                .date_time_original
                .as_ref()
                .and_then(|s| s.parse().ok()),
            latitude: dto.latitude,
            longitude: dto.longitude,
            rating: dto.rating.map(Some),
        };
        let exif_changed = exif_fields.has_updates();

        if exif_changed {
            assets::update_all_exif_fields(&self.pool, &dto.ids, &exif_fields).await?;
        }

        if dto.date_time_relative.is_some() || dto.time_zone.is_some() {
            assets::update_date_time_relative(
                &self.pool,
                &dto.ids,
                dto.date_time_relative,
                dto.time_zone.as_deref(),
            )
            .await?;
        }

        assets::update_all_asset_fields(
            &self.pool,
            &dto.ids,
            dto.is_favorite,
            dto.visibility.as_deref(),
            dto.duplicate_id.map(Some),
        )
        .await?;

        if dto.visibility.as_deref() == Some("locked") {
            assets::remove_assets_from_all_albums(&self.pool, &dto.ids).await?;
        }

        self.jobs.queue_sidecar_write_all(&dto.ids).await?;

        Ok(())
    }

    pub async fn delete_all(&self, auth: &AuthDto, dto: &AssetBulkDeleteReq) -> Result<(), ErrorResp> {
        require_assets_access(&self.pool, auth, &dto.ids, Permission::AssetDelete).await?;
        let force = dto.force.unwrap_or(false);
        assets::trash_assets(&self.pool, &dto.ids, force).await?;

        if force {
            for id in &dto.ids {
                self.websocket.emit_asset_delete(auth.user.id, *id);
            }
        } else {
            let ids: Vec<String> = dto.ids.iter().map(|id| id.to_string()).collect();
            self.websocket.emit_asset_trash(auth.user.id, ids);
        }

        Ok(())
    }

    pub async fn copy(&self, auth: &AuthDto, dto: &AssetCopyReq) -> Result<(), ErrorResp> {
        require_assets_access(
            &self.pool,
            auth,
            &[dto.source_id, dto.target_id],
            Permission::AssetCopy,
        )
        .await?;

        if dto.source_id == dto.target_id {
            return Err(ErrorResp::BadRequest(
                "Source and target id must be distinct".to_string(),
            ));
        }

        let source = assets::get_for_copy(&self.pool, &dto.source_id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Both assets must exist".to_string()))?;
        let target = assets::get_for_copy(&self.pool, &dto.target_id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Both assets must exist".to_string()))?;

        if dto.albums.unwrap_or(true) {
            assets::copy_album_associations(&self.pool, &dto.source_id, &dto.target_id).await?;
        }

        if dto.shared_links.unwrap_or(true) {
            assets::copy_shared_link_associations(&self.pool, &dto.source_id, &dto.target_id)
                .await?;
        }

        if dto.stack.unwrap_or(true) {
            self.copy_stack(&source, &target).await?;
        }

        if dto.favorite.unwrap_or(true) {
            assets::update_asset_fields(
                &self.pool,
                &dto.target_id,
                &AssetUpdateFields {
                    is_favorite: Some(source.is_favorite),
                    visibility: None,
                    live_photo_video_id: None,
                    duplicate_id: None,
                },
            )
            .await?;
        }

        if dto.sidecar.unwrap_or(true) {
            self.copy_sidecar(&source, &target).await?;
        }

        Ok(())
    }

    pub async fn run_jobs(&self, auth: &AuthDto, dto: &AssetJobsReq) -> Result<(), ErrorResp> {
        require_assets_access(&self.pool, auth, &dto.asset_ids, Permission::AssetUpdate).await?;
        self.jobs.run_asset_jobs(&dto.name, &dto.asset_ids).await
    }

    async fn copy_stack(
        &self,
        source: &assets::AssetCopyRow,
        target: &assets::AssetCopyRow,
    ) -> Result<(), ErrorResp> {
        let Some(source_stack_id) = source.stack_id else {
            return Ok(());
        };

        if let Some(target_stack_id) = target.stack_id {
            crate::models::db::stack::merge_stacks(
                &self.pool,
                &source_stack_id,
                &target_stack_id,
            )
            .await?;
            crate::models::db::stack::delete(&self.pool, &source_stack_id).await?;
        } else {
            assets::update_stack_id(&self.pool, &target.id, Some(source_stack_id)).await?;
        }

        Ok(())
    }

    async fn copy_sidecar(
        &self,
        source: &assets::AssetCopyRow,
        target: &assets::AssetCopyRow,
    ) -> Result<(), ErrorResp> {
        let Some(source_path) =
            assets::get_asset_file_path(&self.pool, &source.id, "sidecar").await?
        else {
            return Ok(());
        };

        if let Some(target_path) =
            assets::get_asset_file_path(&self.pool, &target.id, "sidecar").await?
        {
            let _ = tokio::fs::remove_file(&target_path).await;
        }

        let dest_path = format!("{}.xmp", target.original_path);
        tokio::fs::copy(&source_path, &dest_path)
            .await
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?;

        assets::upsert_sidecar_file(&self.pool, &target.id, &dest_path).await?;
        self.jobs
            .queue_asset_extract_metadata_with_source(&target.id, "sidecar-write")
            .await?;

        Ok(())
    }

    pub async fn get_metadata(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
    ) -> Result<Vec<AssetMetadataResponse>, ErrorResp> {
        require_assets_access(&self.pool, auth, &[*asset_id], Permission::AssetRead).await?;
        let rows = crate::models::db::asset_metadata::list_by_asset(&self.pool, asset_id).await?;
        Ok(rows.into_iter().map(map_metadata_row).collect())
    }

    pub async fn get_ocr(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
    ) -> Result<Vec<crate::models::db::asset_ocr::AssetOcrRow>, ErrorResp> {
        require_assets_access(&self.pool, auth, &[*asset_id], Permission::AssetRead).await?;
        crate::models::db::asset_ocr::get_by_asset_id(&self.pool, asset_id)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn get_metadata_by_key(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
        key: &str,
    ) -> Result<AssetMetadataResponse, ErrorResp> {
        require_assets_access(&self.pool, auth, &[*asset_id], Permission::AssetRead).await?;
        let row = crate::models::db::asset_metadata::get_by_key(&self.pool, asset_id, key)
            .await?
            .ok_or_else(|| {
                ErrorResp::BadRequest(format!(
                    "Metadata with key \"{key}\" not found for asset with id \"{asset_id}\""
                ))
            })?;
        Ok(map_metadata_row(row))
    }

    pub async fn upsert_metadata(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
        dto: &AssetMetadataUpsertReq,
    ) -> Result<Vec<AssetMetadataResponse>, ErrorResp> {
        require_assets_access(&self.pool, auth, &[*asset_id], Permission::AssetUpdate).await?;
        validate_unique_keys(&dto.items)?;
        let items: Vec<(String, Value)> = dto
            .items
            .iter()
            .map(|item| (item.key.clone(), item.value.clone()))
            .collect();
        let rows = crate::models::db::asset_metadata::upsert_items(&self.pool, asset_id, &items).await?;
        Ok(rows.into_iter().map(map_metadata_row).collect())
    }

    pub async fn upsert_bulk_metadata(
        &self,
        auth: &AuthDto,
        dto: &AssetMetadataBulkUpsertReq,
    ) -> Result<Vec<AssetMetadataBulkResponse>, ErrorResp> {
        let asset_ids: Vec<Uuid> = dto.items.iter().map(|item| item.asset_id).collect();
        require_assets_access(&self.pool, auth, &asset_ids, Permission::AssetUpdate).await?;

        let mut seen = std::collections::HashSet::new();
        for item in &dto.items {
            let key = format!("({}, {})", item.asset_id, item.key);
            if !seen.insert(key) {
                return Err(ErrorResp::BadRequest(format!(
                    "Duplicate items are not allowed: \"({}, {})\"",
                    item.asset_id, item.key
                )));
            }
        }

        let items: Vec<(Uuid, String, Value)> = dto
            .items
            .iter()
            .map(|item| (item.asset_id, item.key.clone(), item.value.clone()))
            .collect();
        let rows = crate::models::db::asset_metadata::upsert_bulk(&self.pool, &items).await?;
        Ok(rows
            .into_iter()
            .map(|row| AssetMetadataBulkResponse {
                asset_id: row.asset_id,
                key: row.key,
                value: row.value,
                updated_at: row.updated_at.to_rfc3339(),
            })
            .collect())
    }

    pub async fn delete_metadata_by_key(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
        key: &str,
    ) -> Result<(), ErrorResp> {
        require_assets_access(&self.pool, auth, &[*asset_id], Permission::AssetUpdate).await?;
        crate::models::db::asset_metadata::delete_by_key(&self.pool, asset_id, key).await?;
        Ok(())
    }

    pub async fn delete_bulk_metadata(
        &self,
        auth: &AuthDto,
        dto: &AssetMetadataBulkDeleteReq,
    ) -> Result<(), ErrorResp> {
        let asset_ids: Vec<Uuid> = dto.items.iter().map(|item| item.asset_id).collect();
        require_assets_access(&self.pool, auth, &asset_ids, Permission::AssetUpdate).await?;
        let items: Vec<(Uuid, String)> = dto
            .items
            .iter()
            .map(|item| (item.asset_id, item.key.clone()))
            .collect();
        crate::models::db::asset_metadata::delete_bulk(&self.pool, &items).await?;
        Ok(())
    }

    pub async fn get_edits(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
    ) -> Result<AssetEditsResponse, ErrorResp> {
        require_assets_access(&self.pool, auth, &[*asset_id], Permission::AssetEditGet).await?;
        let rows = crate::models::db::asset_edit::list_by_asset(&self.pool, asset_id).await?;
        Ok(AssetEditsResponse {
            asset_id: *asset_id,
            edits: rows.into_iter().map(map_edit_row).collect(),
        })
    }

    pub async fn replace_edits(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
        dto: &AssetEditsCreateReq,
    ) -> Result<AssetEditsResponse, ErrorResp> {
        require_assets_access(&self.pool, auth, &[*asset_id], Permission::AssetEditCreate).await?;
        let edits: Vec<(String, Value)> = dto
            .edits
            .iter()
            .map(|edit| (edit.action.clone(), edit.parameters.clone()))
            .collect();
        let rows = crate::models::db::asset_edit::replace_all(&self.pool, asset_id, &edits).await?;
        let _ = self.jobs.queue_asset_edit_thumbnails(asset_id).await;
        Ok(AssetEditsResponse {
            asset_id: *asset_id,
            edits: rows.into_iter().map(map_edit_row).collect(),
        })
    }

    pub async fn delete_edits(&self, auth: &AuthDto, asset_id: &Uuid) -> Result<(), ErrorResp> {
        require_assets_access(&self.pool, auth, &[*asset_id], Permission::AssetEditDelete).await?;
        crate::models::db::asset_edit::delete_all(&self.pool, asset_id).await?;
        let _ = self.jobs.queue_asset_edit_thumbnails(asset_id).await;
        Ok(())
    }
}

fn map_stats(stats: &AssetStatsRow) -> AssetStatsResponse {
    AssetStatsResponse {
        images: stats.image,
        videos: stats.video,
        total: stats.image + stats.video + stats.audio + stats.other,
    }
}

fn map_metadata_row(row: crate::models::db::asset_metadata::AssetMetadataRow) -> AssetMetadataResponse {
    AssetMetadataResponse {
        key: row.key,
        value: row.value,
        updated_at: row.updated_at.to_rfc3339(),
    }
}

fn map_edit_row(row: crate::models::db::asset_edit::AssetEditRow) -> AssetEditResponse {
    AssetEditResponse {
        id: row.id,
        action: row.action,
        parameters: row.parameters,
    }
}

fn validate_unique_keys(items: &[AssetMetadataItemReq]) -> Result<(), ErrorResp> {
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if !seen.insert(&item.key) {
            return Err(ErrorResp::BadRequest(format!(
                "Duplicate items are not allowed: \"{}\"",
                item.key
            )));
        }
    }
    Ok(())
}

fn build_exif_fields(dto: &UpdateAssetReq) -> ExifUpdateFields {
    ExifUpdateFields {
        description: dto.description.clone(),
        date_time_original: dto
            .date_time_original
            .as_ref()
            .and_then(|s| s.parse().ok()),
        latitude: dto.latitude,
        longitude: dto.longitude,
        rating: dto.rating.map(Some),
    }
}

fn validate_lat_lon(latitude: Option<f64>, longitude: Option<f64>) -> Result<(), ErrorResp> {
    match (latitude, longitude) {
        (Some(_), None) | (None, Some(_)) => Err(ErrorResp::BadRequest(
            "Latitude and longitude must be provided together".to_string(),
        )),
        _ => Ok(()),
    }
}

async fn on_before_link(
    pool: &PgPool,
    user_id: &Uuid,
    live_photo_video_id: &Uuid,
) -> Result<Option<Uuid>, ErrorResp> {
    let motion = assets::get_basic_by_id(pool, live_photo_video_id)
        .await?
        .ok_or_else(|| ErrorResp::BadRequest("Live photo video not found".to_string()))?;

    if motion.asset_type != "VIDEO" {
        return Err(ErrorResp::BadRequest(
            "Live photo video must be a video".to_string(),
        ));
    }
    if motion.owner_id != *user_id {
        return Err(ErrorResp::BadRequest(
            "Live photo video does not belong to the user".to_string(),
        ));
    }

    if motion.visibility == "timeline" {
        assets::update_asset_fields(
            pool,
            live_photo_video_id,
            &AssetUpdateFields {
                is_favorite: None,
                visibility: Some("hidden".to_string()),
                live_photo_video_id: None,
                duplicate_id: None,
            },
        )
        .await?;
        Ok(Some(*live_photo_video_id))
    } else {
        Ok(None)
    }
}

async fn on_before_unlink(
    pool: &PgPool,
    live_photo_video_id: &Uuid,
) -> Result<Option<AssetBasicRow>, ErrorResp> {
    let motion = match assets::get_basic_by_id(pool, live_photo_video_id).await? {
        Some(motion) => motion,
        None => return Ok(None),
    };

    if assets::is_android_motion_path(&motion.original_path) {
        return Err(ErrorResp::BadRequest(
            "Cannot unlink Android motion photos".to_string(),
        ));
    }

    Ok(Some(motion))
}

async fn on_after_unlink(
    pool: &PgPool,
    user_id: &Uuid,
    live_photo_video_id: &Uuid,
    visibility: &str,
) -> Result<(), ErrorResp> {
    let _ = user_id;
    assets::update_asset_fields(
        pool,
        live_photo_video_id,
        &AssetUpdateFields {
            is_favorite: None,
            visibility: Some(visibility.to_string()),
            live_photo_video_id: None,
            duplicate_id: None,
        },
    )
    .await?;
    Ok(())
}
