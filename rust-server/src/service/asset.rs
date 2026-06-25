use sqlx::PgPool;
use uuid::Uuid;

use crate::service::job::JobService;
use crate::models::db::assets::{
    self, AssetBasicRow, AssetUpdateFields, ExifUpdateFields, AssetStatsRow,
};
use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::{map_asset, AssetResponse, AssetStatsResponse};
use crate::models::response::response::ErrorResp;
use crate::service::access::require_assets_access;
use crate::utils::permission::require_permission;
use crate::utils::query::parse_query_bool;

#[derive(Clone)]
pub struct AssetService {
    pool: PgPool,
    jobs: JobService,
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

impl AssetService {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self { pool, jobs }
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

        let stack = if let Some(stack_id) = row.stack_id {
            assets::get_stack(&self.pool, &stack_id).await?
        } else {
            None
        };

        let strip_metadata = auth
            .shared_link
            .as_ref()
            .is_some_and(|link| !link.show_exif);

        Ok(map_asset(
            &row,
            stack.as_ref(),
            auth,
            strip_metadata,
        ))
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
                on_before_link(&self.pool, &auth.user.id, video_id).await?;
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
        assets::trash_assets(&self.pool, &dto.ids, dto.force.unwrap_or(false)).await?;
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
) -> Result<(), ErrorResp> {
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
    }

    Ok(())
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
