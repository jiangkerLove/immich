use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::models::db::assets;
use crate::models::db::memory::{
    self, MemoryCreateData, MemoryRow, MemorySearchFilter, MemoryUpdateData,
};
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::map_assets;
use crate::models::response::memory::{
    format_datetime, format_optional_datetime, parse_memory_data, MemoryResponse,
    MemoryStatisticsResponse,
};
use crate::models::response::response::ErrorResp;
use crate::service::access::require_assets_access;
use crate::service::album::{BulkIdErrorReason, BulkIdResponse, BulkIdsReq};
use crate::utils::permission::require_permission;
use crate::models::db::auth_permission::Permission;

#[derive(Clone)]
pub struct MemoryService {
    pool: PgPool,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchQuery {
    #[serde(rename = "for")]
    pub for_date: Option<String>,
    pub is_trashed: Option<String>,
    pub is_saved: Option<String>,
    pub is_upcoming: Option<String>,
    pub r#type: Option<String>,
    pub size: Option<i64>,
    pub page: Option<i64>,
    pub order: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDataDto {
    pub year: i32,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCreateReq {
    #[serde(rename = "type")]
    pub memory_type: String,
    pub data: MemoryDataDto,
    pub memory_at: DateTime<Utc>,
    #[serde(default)]
    pub asset_ids: Vec<Uuid>,
    pub is_saved: Option<bool>,
    pub seen_at: Option<DateTime<Utc>>,
    pub show_at: Option<DateTime<Utc>>,
    pub hide_at: Option<DateTime<Utc>>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MemoryUpdateReq {
    pub is_saved: Option<bool>,
    pub memory_at: Option<DateTime<Utc>>,
    pub seen_at: Option<DateTime<Utc>>,
}

impl MemoryService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn search(
        &self,
        auth: &AuthDto,
        query: &MemorySearchQuery,
    ) -> Result<Vec<MemoryResponse>, ErrorResp> {
        require_permission(auth, Permission::MemoryRead)?;
        let filter = build_filter(query)?;
        let memories = memory::search_memories(&self.pool, &auth.user.id, &filter).await?;
        map_memories(&self.pool, memories, auth).await
    }

    pub async fn statistics(
        &self,
        auth: &AuthDto,
        query: &MemorySearchQuery,
    ) -> Result<MemoryStatisticsResponse, ErrorResp> {
        require_permission(auth, Permission::MemoryStatistics)?;
        let filter = build_filter(query)?;
        let total = memory::count_memories(&self.pool, &auth.user.id, &filter).await?;
        Ok(MemoryStatisticsResponse { total })
    }

    pub async fn get(
        &self,
        auth: &AuthDto,
        id: &Uuid,
    ) -> Result<MemoryResponse, ErrorResp> {
        require_memory_access(&self.pool, auth, id, Permission::MemoryRead).await?;
        let memory = memory::get_by_id(&self.pool, id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Memory not found".to_string()))?;
        map_memory(&self.pool, memory, auth).await
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &MemoryCreateReq,
    ) -> Result<MemoryResponse, ErrorResp> {
        require_permission(auth, Permission::MemoryCreate)?;

        let asset_ids = dto.asset_ids.clone();
        if !asset_ids.is_empty() {
            require_assets_access(&self.pool, auth, &asset_ids, Permission::AssetUpdate).await?;
        }

        let memory_id = memory::create(
            &self.pool,
            &MemoryCreateData {
                owner_id: auth.user.id,
                memory_type: dto.memory_type.clone(),
                data: serde_json::json!({ "year": dto.data.year }),
                is_saved: dto.is_saved.unwrap_or(false),
                memory_at: dto.memory_at,
                seen_at: dto.seen_at,
                show_at: dto.show_at,
                hide_at: dto.hide_at,
            },
            &asset_ids,
        )
        .await?;

        self.get(auth, &memory_id).await
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &MemoryUpdateReq,
    ) -> Result<MemoryResponse, ErrorResp> {
        require_memory_access(&self.pool, auth, id, Permission::MemoryUpdate).await?;

        if dto.is_saved.is_none() && dto.memory_at.is_none() && dto.seen_at.is_none() {
            return Err(ErrorResp::BadRequest("No fields to update".to_string()));
        }

        memory::update(
            &self.pool,
            id,
            &MemoryUpdateData {
                is_saved: dto.is_saved,
                memory_at: dto.memory_at,
                seen_at: dto.seen_at,
            },
        )
        .await?;

        self.get(auth, id).await
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_memory_access(&self.pool, auth, id, Permission::MemoryDelete).await?;
        memory::delete(&self.pool, id).await?;
        Ok(())
    }

    pub async fn add_assets(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &BulkIdsReq,
    ) -> Result<Vec<BulkIdResponse>, ErrorResp> {
        require_permission(auth, Permission::MemoryAssetCreate)?;
        require_memory_access(&self.pool, auth, id, Permission::MemoryRead).await?;

        let existing = memory::filter_asset_ids_in_memory(&self.pool, id, &dto.ids).await?;
        let existing_set: HashSet<Uuid> = existing.iter().copied().collect();
        let not_present: Vec<Uuid> = dto
            .ids
            .iter()
            .filter(|asset_id| !existing_set.contains(asset_id))
            .copied()
            .collect();

        let allowed: HashSet<Uuid> = if not_present.is_empty() {
            HashSet::new()
        } else {
            filter_update_accessible_ids(&self.pool, auth, &not_present)
                .await?
                .into_iter()
                .collect()
        };

        let mut results = Vec::with_capacity(dto.ids.len());
        let mut new_asset_ids = Vec::new();

        for asset_id in &dto.ids {
            if existing_set.contains(asset_id) {
                results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::Duplicate),
                });
                continue;
            }

            if !allowed.contains(asset_id) {
                results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NoPermission),
                });
                continue;
            }

            new_asset_ids.push(*asset_id);
            results.push(BulkIdResponse {
                id: *asset_id,
                success: true,
                error: None,
            });
        }

        if !new_asset_ids.is_empty() {
            memory::add_asset_ids(&self.pool, id, &new_asset_ids).await?;
            memory::touch_updated_at(&self.pool, id).await?;
        }

        Ok(results)
    }

    pub async fn remove_assets(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &BulkIdsReq,
    ) -> Result<Vec<BulkIdResponse>, ErrorResp> {
        require_permission(auth, Permission::MemoryAssetDelete)?;
        require_memory_access(&self.pool, auth, id, Permission::MemoryUpdate).await?;

        let existing = memory::filter_asset_ids_in_memory(&self.pool, id, &dto.ids).await?;
        let existing_set: HashSet<Uuid> = existing.iter().copied().collect();
        let can_always_remove =
            memory::owner_has_memory(&self.pool, &auth.user.id, id).await?;

        let allowed: HashSet<Uuid> = if can_always_remove {
            existing_set.clone()
        } else {
            filter_share_accessible_ids(&self.pool, auth, &existing).await?
                .into_iter()
                .collect()
        };

        let mut results = Vec::with_capacity(dto.ids.len());
        let mut removed_ids = Vec::new();

        for asset_id in &dto.ids {
            if !existing_set.contains(asset_id) {
                results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NotFound),
                });
                continue;
            }

            if !allowed.contains(asset_id) {
                results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NoPermission),
                });
                continue;
            }

            removed_ids.push(*asset_id);
            results.push(BulkIdResponse {
                id: *asset_id,
                success: true,
                error: None,
            });
        }

        if !removed_ids.is_empty() {
            memory::remove_asset_ids(&self.pool, id, &removed_ids).await?;
            memory::touch_updated_at(&self.pool, id).await?;
        }

        Ok(results)
    }
}

async fn require_memory_access(
    pool: &PgPool,
    auth: &AuthDto,
    memory_id: &Uuid,
    permission: Permission,
) -> Result<(), ErrorResp> {
    require_permission(auth, permission)?;
    if !memory::owner_has_memory(pool, &auth.user.id, memory_id).await? {
        return Err(ErrorResp::BadRequest("Memory not found".to_string()));
    }
    Ok(())
}

async fn filter_update_accessible_ids(
    pool: &PgPool,
    auth: &AuthDto,
    asset_ids: &[Uuid],
) -> Result<Vec<Uuid>, ErrorResp> {
    require_permission(auth, Permission::AssetUpdate)?;
    let elevated = auth
        .session
        .as_ref()
        .is_some_and(|session| session.has_elevated_permission);
    Ok(assets::filter_accessible_ids(pool, &auth.user.id, asset_ids, elevated, true).await?)
}

async fn filter_share_accessible_ids(
    pool: &PgPool,
    auth: &AuthDto,
    asset_ids: &[Uuid],
) -> Result<Vec<Uuid>, ErrorResp> {
    if asset_ids.is_empty() {
        return Ok(vec![]);
    }
    let elevated = auth
        .session
        .as_ref()
        .is_some_and(|session| session.has_elevated_permission);
    Ok(assets::filter_accessible_ids(pool, &auth.user.id, asset_ids, elevated, false).await?)
}

fn build_filter(query: &MemorySearchQuery) -> Result<MemorySearchFilter, ErrorResp> {
    Ok(MemorySearchFilter {
        for_date: query
            .for_date
            .as_deref()
            .map(parse_datetime)
            .transpose()?,
        is_trashed: parse_bool(&query.is_trashed),
        is_saved: parse_bool(&query.is_saved),
        memory_type: query.r#type.clone(),
        size: query.size,
        page: query.page,
        is_upcoming: parse_bool(&query.is_upcoming),
        order: query.order.clone(),
    })
}

async fn map_memories(
    pool: &PgPool,
    memories: Vec<MemoryRow>,
    auth: &AuthDto,
) -> Result<Vec<MemoryResponse>, ErrorResp> {
    if memories.is_empty() {
        return Ok(vec![]);
    }

    let memory_ids: Vec<Uuid> = memories.iter().map(|memory| memory.id).collect();
    let memory_assets = memory::get_memory_assets(pool, &memory_ids).await?;

    let mut assets_by_memory: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for row in memory_assets {
        assets_by_memory
            .entry(row.memory_id)
            .or_default()
            .push(row.asset_id);
    }

    let all_asset_ids: Vec<Uuid> = assets_by_memory
        .values()
        .flat_map(|ids| ids.iter().copied())
        .collect();
    let asset_rows = assets::get_details_by_ids(pool, &all_asset_ids).await?;
    let mapped_assets = map_assets(pool, &asset_rows, auth, false).await?;
    let asset_map: HashMap<Uuid, _> = mapped_assets
        .into_iter()
        .map(|asset| (asset.id, asset))
        .collect();

    let mut responses = Vec::new();
    for memory in memories {
        let response = build_memory_response(&memory, &assets_by_memory, &asset_map);
        if !response.assets.is_empty() {
            responses.push(response);
        }
    }

    Ok(responses)
}

async fn map_memory(
    pool: &PgPool,
    memory: MemoryRow,
    auth: &AuthDto,
) -> Result<MemoryResponse, ErrorResp> {
    let memory_assets = memory::get_memory_assets(pool, &[memory.id]).await?;
    let mut assets_by_memory: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for row in memory_assets {
        assets_by_memory
            .entry(row.memory_id)
            .or_default()
            .push(row.asset_id);
    }

    let all_asset_ids: Vec<Uuid> = assets_by_memory
        .values()
        .flat_map(|ids| ids.iter().copied())
        .collect();
    let asset_rows = assets::get_details_by_ids(pool, &all_asset_ids).await?;
    let mapped_assets = map_assets(pool, &asset_rows, auth, false).await?;
    let asset_map: HashMap<Uuid, _> = mapped_assets
        .into_iter()
        .map(|asset| (asset.id, asset))
        .collect();

    Ok(build_memory_response(&memory, &assets_by_memory, &asset_map))
}

fn build_memory_response(
    memory: &MemoryRow,
    assets_by_memory: &HashMap<Uuid, Vec<Uuid>>,
    asset_map: &HashMap<Uuid, crate::models::response::asset::AssetResponse>,
) -> MemoryResponse {
    let asset_ids = assets_by_memory
        .get(&memory.id)
        .cloned()
        .unwrap_or_default();

    let assets: Vec<_> = asset_ids
        .iter()
        .filter_map(|asset_id| asset_map.get(asset_id).cloned())
        .collect();

    MemoryResponse {
        id: memory.id,
        created_at: format_datetime(&memory.created_at),
        updated_at: format_datetime(&memory.updated_at),
        deleted_at: format_optional_datetime(&memory.deleted_at),
        memory_at: format_datetime(&memory.memory_at),
        seen_at: format_optional_datetime(&memory.seen_at),
        show_at: format_optional_datetime(&memory.show_at),
        hide_at: format_optional_datetime(&memory.hide_at),
        owner_id: memory.owner_id,
        memory_type: memory.memory_type.clone(),
        data: parse_memory_data(&memory.data),
        is_saved: memory.is_saved,
        assets,
    }
}

fn parse_bool(value: &Option<String>) -> Option<bool> {
    value
        .as_deref()
        .and_then(crate::utils::query::parse_query_bool)
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>, ErrorResp> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| {
            value
                .parse::<DateTime<Utc>>()
                .map_err(|_| ErrorResp::BadRequest("Invalid datetime".to_string()))
        })
}
