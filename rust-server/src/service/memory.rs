use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::models::db::assets::get_details_by_ids;
use crate::models::db::memory::{get_memory_assets, search_memories, MemorySearchFilter};
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::map_asset;
use crate::models::response::memory::{
    format_datetime, format_optional_datetime, parse_memory_data, MemoryResponse,
};
use crate::models::response::response::ErrorResp;
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
    pub r#type: Option<String>,
    pub size: Option<i64>,
    pub order: Option<String>,
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

        let filter = MemorySearchFilter {
            for_date: query
                .for_date
                .as_deref()
                .map(parse_datetime)
                .transpose()?,
            is_trashed: parse_bool(&query.is_trashed),
            is_saved: parse_bool(&query.is_saved),
            memory_type: query.r#type.clone(),
            size: query.size,
            order: query.order.clone(),
        };

        let memories = search_memories(&self.pool, &auth.user.id, &filter).await?;
        if memories.is_empty() {
            return Ok(vec![]);
        }

        let memory_ids: Vec<Uuid> = memories.iter().map(|memory| memory.id).collect();
        let memory_assets = get_memory_assets(&self.pool, &memory_ids).await?;

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
        let asset_rows = get_details_by_ids(&self.pool, &all_asset_ids).await?;
        let asset_map: HashMap<Uuid, _> = asset_rows.into_iter().map(|row| (row.id, row)).collect();

        let mut responses = Vec::new();
        for memory in memories {
            let asset_ids = assets_by_memory.get(&memory.id).cloned().unwrap_or_default();
            if asset_ids.is_empty() {
                continue;
            }

            let assets: Vec<crate::models::response::asset::AssetResponse> = asset_ids
                .iter()
                .filter_map(|asset_id| asset_map.get(asset_id))
                .map(|row| map_asset(row, None, auth, false))
                .collect();

            if assets.is_empty() {
                continue;
            }

            responses.push(MemoryResponse {
                id: memory.id,
                created_at: format_datetime(&memory.created_at),
                updated_at: format_datetime(&memory.updated_at),
                deleted_at: format_optional_datetime(&memory.deleted_at),
                memory_at: format_datetime(&memory.memory_at),
                seen_at: format_optional_datetime(&memory.seen_at),
                show_at: format_optional_datetime(&memory.show_at),
                hide_at: format_optional_datetime(&memory.hide_at),
                owner_id: memory.owner_id,
                memory_type: memory.memory_type,
                data: parse_memory_data(&memory.data),
                is_saved: memory.is_saved,
                assets,
            });
        }

        Ok(responses)
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
