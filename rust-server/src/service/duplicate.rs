use serde::Serialize;
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

use crate::models::db::album;
use crate::models::db::assets::{self, ExifUpdateFields};
use crate::models::db::auth_permission::Permission;
use crate::models::db::duplicate;
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::{AssetResponse, map_assets};
use crate::models::response::response::ErrorResp;
use crate::service::access::require_assets_access;
use crate::service::album::{BulkIdErrorReason, BulkIdResponse, BulkIdsReq};
use crate::service::job::JobService;
use crate::service::websocket::WebSocketHub;
use crate::utils::duplicate::suggest_duplicate_keep_asset_ids;
use crate::utils::permission::require_permission;
use crate::utils::system_config::get_merged;

#[derive(Clone)]
pub struct DuplicateService {
    pool: PgPool,
    jobs: JobService,
    websocket: WebSocketHub,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateResponse {
    pub duplicate_id: Uuid,
    pub assets: Vec<AssetResponse>,
    pub suggested_keep_asset_ids: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateResolveGroupReq {
    pub duplicate_id: Uuid,
    pub keep_asset_ids: Vec<Uuid>,
    pub trash_asset_ids: Vec<Uuid>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateResolveReq {
    pub groups: Vec<DuplicateResolveGroupReq>,
}

impl DuplicateService {
    pub fn new(pool: PgPool, jobs: JobService, websocket: WebSocketHub) -> Self {
        Self {
            pool,
            jobs,
            websocket,
        }
    }

    pub async fn get_all(&self, auth: &AuthDto) -> Result<Vec<DuplicateResponse>, ErrorResp> {
        require_permission(auth, Permission::DuplicateRead)?;

        duplicate::cleanup_singleton_groups(&self.pool, &auth.user.id).await?;

        let groups = duplicate::list_duplicate_ids(&self.pool, &auth.user.id).await?;
        let mut responses = Vec::with_capacity(groups.len());

        for group in groups {
            let asset_ids =
                duplicate::list_asset_ids_by_duplicate_id(&self.pool, &group.duplicate_id).await?;
            if asset_ids.len() < 2 {
                continue;
            }

            let rows = assets::get_details_by_ids(&self.pool, &asset_ids).await?;
            let mapped = map_assets(&self.pool, &rows, auth, false).await?;
            responses.push(DuplicateResponse {
                duplicate_id: group.duplicate_id,
                suggested_keep_asset_ids: suggest_duplicate_keep_asset_ids(&mapped),
                assets: mapped,
            });
        }

        Ok(responses)
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::DuplicateDelete)?;
        if !duplicate::duplicate_group_exists(&self.pool, id).await? {
            return Err(ErrorResp::BadRequest(
                "Duplicate group not found".to_string(),
            ));
        }
        duplicate::clear_duplicate_group(&self.pool, &auth.user.id, id).await?;
        Ok(())
    }

    pub async fn delete_all(&self, auth: &AuthDto, dto: &BulkIdsReq) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::DuplicateDelete)?;
        duplicate::clear_duplicate_groups(&self.pool, &auth.user.id, &dto.ids).await?;
        Ok(())
    }

    pub async fn resolve(
        &self,
        auth: &AuthDto,
        dto: &DuplicateResolveReq,
    ) -> Result<Vec<BulkIdResponse>, ErrorResp> {
        require_permission(auth, Permission::DuplicateDelete)?;

        let mut results = Vec::with_capacity(dto.groups.len());
        for group in &dto.groups {
            results.push(self.resolve_group(auth, group).await);
        }
        Ok(results)
    }

    async fn resolve_group(
        &self,
        auth: &AuthDto,
        group: &DuplicateResolveGroupReq,
    ) -> BulkIdResponse {
        let duplicate_id = group.duplicate_id;

        let asset_ids =
            match duplicate::list_asset_ids_by_duplicate_id(&self.pool, &duplicate_id).await {
                Ok(ids) if !ids.is_empty() => ids,
                _ => {
                    return BulkIdResponse {
                        id: duplicate_id,
                        success: false,
                        error: Some(BulkIdErrorReason::NotFound),
                    };
                }
            };

        let group_asset_ids: HashSet<Uuid> = asset_ids.iter().copied().collect();
        let ids_to_keep: Vec<Uuid> = group
            .keep_asset_ids
            .iter()
            .filter(|id| group_asset_ids.contains(id))
            .copied()
            .collect();
        let ids_to_trash: Vec<Uuid> = group
            .trash_asset_ids
            .iter()
            .filter(|id| group_asset_ids.contains(id))
            .copied()
            .collect();

        for asset_id in &group_asset_ids {
            if ids_to_keep.contains(asset_id) && ids_to_trash.contains(asset_id) {
                return BulkIdResponse {
                    id: duplicate_id,
                    success: false,
                    error: Some(BulkIdErrorReason::Validation),
                };
            }
            if !ids_to_keep.contains(asset_id) && !ids_to_trash.contains(asset_id) {
                return BulkIdResponse {
                    id: duplicate_id,
                    success: false,
                    error: Some(BulkIdErrorReason::Validation),
                };
            }
        }

        if !ids_to_trash.is_empty() {
            if require_assets_access(&self.pool, auth, &ids_to_trash, Permission::AssetDelete)
                .await
                .is_err()
            {
                return BulkIdResponse {
                    id: duplicate_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NoPermission),
                };
            }
        }

        if ids_to_keep.len() == 1 && !ids_to_trash.is_empty() {
            let rows = match assets::get_details_by_ids(&self.pool, &asset_ids).await {
                Ok(rows) => rows,
                Err(_) => {
                    return BulkIdResponse {
                        id: duplicate_id,
                        success: false,
                        error: Some(BulkIdErrorReason::Unknown),
                    };
                }
            };
            let mapped = match map_assets(&self.pool, &rows, auth, false).await {
                Ok(items) => items,
                Err(_) => {
                    return BulkIdResponse {
                        id: duplicate_id,
                        success: false,
                        error: Some(BulkIdErrorReason::Unknown),
                    };
                }
            };

            let merge = compute_merge(&mapped, &self.pool, auth, &group_asset_ids).await;
            if let Ok(merge) = merge {
                for album_id in &merge.album_ids {
                    let _ = album::add_asset_ids(&self.pool, album_id, &ids_to_keep).await;
                }

                if !merge.tag_ids.is_empty() {
                    for asset_id in &ids_to_keep {
                        replace_asset_tags(asset_id, &merge.tag_ids, &self.pool).await;
                    }
                    update_exif_tags(&self.pool, &ids_to_keep, &merge.tag_values).await;
                }

                let has_exif = merge.exif.description.is_some()
                    || merge.exif.rating.is_some()
                    || merge.exif.latitude.is_some();
                if has_exif {
                    let _ =
                        assets::update_all_exif_fields(&self.pool, &ids_to_keep, &merge.exif).await;
                }

                if has_exif || !merge.tag_ids.is_empty() {
                    let _ = self.jobs.queue_sidecar_write_all(&ids_to_keep).await;
                }

                let _ = assets::update_all_asset_fields(
                    &self.pool,
                    &ids_to_keep,
                    Some(merge.is_favorite),
                    merge.visibility.as_deref(),
                    Some(None),
                )
                .await;
            }
        } else if !ids_to_keep.is_empty() {
            let _ =
                assets::update_all_asset_fields(&self.pool, &ids_to_keep, None, None, Some(None))
                    .await;
        }

        if !ids_to_trash.is_empty() {
            let _ =
                assets::update_all_asset_fields(&self.pool, &ids_to_trash, None, None, Some(None))
                    .await;

            // Match TS DuplicateService.resolveGroup: force when trash is disabled.
            let trash_enabled = get_merged(&self.pool)
                .await
                .ok()
                .and_then(|config| {
                    config
                        .get("trash")
                        .and_then(|value| value.get("enabled"))
                        .and_then(|value| value.as_bool())
                })
                .unwrap_or(true);
            let is_force = !trash_enabled;

            let _ = assets::trash_assets(&self.pool, &ids_to_trash, is_force).await;
            if is_force {
                let _ = self.jobs.queue_asset_empty_trash().await;
            } else {
                let ids: Vec<String> = ids_to_trash.iter().map(|id| id.to_string()).collect();
                self.websocket.emit_asset_trash(auth.user.id, ids);
            }
        }

        BulkIdResponse {
            id: duplicate_id,
            success: true,
            error: None,
        }
    }
}

struct MergeResult {
    is_favorite: bool,
    visibility: Option<String>,
    exif: ExifUpdateFields,
    album_ids: Vec<Uuid>,
    tag_ids: Vec<Uuid>,
    tag_values: Vec<String>,
}

async fn compute_merge(
    assets_list: &[AssetResponse],
    pool: &PgPool,
    auth: &AuthDto,
    group_asset_ids: &HashSet<Uuid>,
) -> Result<MergeResult, sqlx::Error> {
    let is_favorite = assets_list.iter().any(|asset| asset.is_favorite);

    let visibility_order = ["locked", "archive", "timeline", "hidden"];
    let visibility = visibility_order
        .iter()
        .find(|level| assets_list.iter().any(|asset| asset.visibility == **level))
        .map(|level| level.to_string());

    let mut rating = 0i32;
    for asset in assets_list {
        if let Some(value) = asset
            .exif_info
            .as_ref()
            .and_then(|exif| exif.get("rating"))
            .and_then(|value| value.as_i64())
        {
            rating = rating.max(value as i32);
        }
    }

    let description = unique_description_lines(assets_list);
    let latitude = unique_coordinate(assets_list, "latitude");
    let longitude = unique_coordinate(assets_list, "longitude");

    let mut album_ids = HashSet::new();
    for asset_id in group_asset_ids {
        let ids = album::list_album_ids_by_asset(pool, &auth.user.id, asset_id).await?;
        album_ids.extend(ids);
    }

    let mut tag_ids = HashSet::new();
    let mut tag_values = HashSet::new();
    for asset in assets_list {
        if let Some(tags) = &asset.tags {
            for tag in tags {
                tag_ids.insert(tag.id);
                tag_values.insert(tag.value.clone());
            }
        }
    }

    Ok(MergeResult {
        is_favorite,
        visibility,
        exif: ExifUpdateFields {
            description,
            rating: if rating > 0 { Some(Some(rating)) } else { None },
            latitude,
            longitude,
            ..Default::default()
        },
        album_ids: album_ids.into_iter().collect(),
        tag_ids: tag_ids.into_iter().collect(),
        tag_values: tag_values.into_iter().collect(),
    })
}

fn unique_description_lines(assets_list: &[AssetResponse]) -> Option<String> {
    let mut unique = HashSet::new();
    let mut lines = Vec::new();
    for asset in assets_list {
        if let Some(text) = asset
            .exif_info
            .as_ref()
            .and_then(|exif| exif.get("description"))
            .and_then(|value| value.as_str())
        {
            for line in text.split('\n') {
                let trimmed = line.trim();
                if !trimmed.is_empty() && unique.insert(trimmed.to_string()) {
                    lines.push(trimmed.to_string());
                }
            }
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn unique_coordinate(assets_list: &[AssetResponse], key: &str) -> Option<f64> {
    let mut values: Vec<f64> = assets_list
        .iter()
        .filter_map(|asset| {
            asset
                .exif_info
                .as_ref()
                .and_then(|exif| exif.get(key))
                .and_then(|coord| coord.as_f64())
        })
        .collect();

    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values.dedup_by(|a, b| (*a - *b).abs() < f64::EPSILON);

    if values.len() == 1 {
        Some(values[0])
    } else {
        None
    }
}

async fn replace_asset_tags(asset_id: &Uuid, tag_ids: &[Uuid], pool: &PgPool) {
    let _ = sqlx::query(r#"DELETE FROM tag_asset WHERE "assetId" = $1"#)
        .bind(asset_id)
        .execute(pool)
        .await;
    for tag_id in tag_ids {
        let _ = sqlx::query(
            r#"INSERT INTO tag_asset ("tagId", "assetId") VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
        )
        .bind(tag_id)
        .bind(asset_id)
        .execute(pool)
        .await;
    }
}

async fn update_exif_tags(pool: &PgPool, asset_ids: &[Uuid], tag_values: &[String]) {
    if asset_ids.is_empty() {
        return;
    }
    let _ = sqlx::query(r#"UPDATE asset_exif SET tags = $1 WHERE "assetId" = ANY($2)"#)
        .bind(tag_values)
        .bind(asset_ids)
        .execute(pool)
        .await;
}
