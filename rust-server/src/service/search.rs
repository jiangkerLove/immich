use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::assets;
use crate::models::db::auth_permission::Permission;
use crate::models::db::person;
use crate::models::db::search::{self, PlaceRow, SearchFilter, SearchPage};
use crate::models::db::system_metadata::{get_machine_learning_config, is_smart_search_enabled};
use crate::models::db::timeline::get_timeline_partner_ids;
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::{map_asset, AssetResponse};
use crate::models::response::response::ErrorResp;
use crate::models::response::search::{
    empty_search_response, map_person, PersonResponse, PlacesResponse, SearchExploreResponse,
    SearchResponse, SearchStatisticsResponse,
};
use crate::service::access::require_asset_access;
use crate::service::ml::encode_clip_text;
use crate::utils::permission::require_permission;

#[derive(Clone)]
pub struct SearchService {
    pool: PgPool,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseSearchReq {
    pub library_id: Option<Uuid>,
    #[serde(rename = "type")]
    pub asset_type: Option<String>,
    pub is_encoded: Option<bool>,
    pub is_favorite: Option<bool>,
    pub is_motion: Option<bool>,
    pub is_offline: Option<bool>,
    pub visibility: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
    pub updated_before: Option<DateTime<Utc>>,
    pub updated_after: Option<DateTime<Utc>>,
    pub trashed_before: Option<DateTime<Utc>>,
    pub trashed_after: Option<DateTime<Utc>>,
    pub taken_before: Option<DateTime<Utc>>,
    pub taken_after: Option<DateTime<Utc>>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens_model: Option<String>,
    pub is_not_in_album: Option<bool>,
    pub person_ids: Option<Vec<Uuid>>,
    #[serde(default)]
    pub tag_ids: Option<Option<Vec<Uuid>>>,
    pub album_ids: Option<Vec<Uuid>>,
    pub rating: Option<i32>,
    pub ocr: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultSearchReq {
    #[serde(flatten)]
    pub base: BaseSearchReq,
    pub with_deleted: Option<bool>,
    pub with_exif: Option<bool>,
    pub with_stacked: Option<bool>,
    pub with_people: Option<bool>,
    pub size: Option<i64>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataSearchReq {
    #[serde(flatten)]
    pub result: ResultSearchReq,
    pub id: Option<Uuid>,
    pub description: Option<String>,
    pub checksum: Option<String>,
    pub original_file_name: Option<String>,
    pub original_path: Option<String>,
    pub preview_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub encoded_video_path: Option<String>,
    pub order: Option<String>,
    pub page: Option<i64>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsSearchReq {
    #[serde(flatten)]
    pub base: BaseSearchReq,
    pub description: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RandomSearchReq {
    #[serde(flatten)]
    pub result: ResultSearchReq,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeAssetSearchReq {
    #[serde(flatten)]
    pub result: ResultSearchReq,
    pub min_file_size: Option<i64>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSearchReq {
    #[serde(flatten)]
    pub result: ResultSearchReq,
    pub query: Option<String>,
    pub query_asset_id: Option<Uuid>,
    pub language: Option<String>,
    pub page: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPeopleQuery {
    pub name: String,
    pub with_hidden: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPlacesQuery {
    pub name: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct SearchSuggestionQuery {
    #[serde(rename = "type")]
    pub suggestion_type: String,
    pub country: Option<String>,
    pub state: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens_model: Option<String>,
    pub include_null: Option<String>,
}

impl SearchService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn search_metadata(
        &self,
        auth: &AuthDto,
        dto: &MetadataSearchReq,
    ) -> Result<SearchResponse, ErrorResp> {
        require_permission(auth, Permission::AssetRead)?;
        self.require_locked_access(auth, dto.result.base.visibility.as_deref())?;

        let user_ids = self
            .get_user_ids(auth, dto.result.base.visibility.as_deref())
            .await?;
        let mut filter = build_filter_from_result(&dto.result, user_ids);
        filter.asset_id = dto.id;
        filter.description = dto.description.clone();
        filter.original_file_name = dto.original_file_name.clone();
        filter.original_path = dto.original_path.clone();
        filter.encoded_video_path = dto.encoded_video_path.clone();
        filter.checksum = dto.checksum.as_deref().and_then(decode_checksum);

        let page = dto.page.unwrap_or(1).max(1);
        let size = dto.result.size.unwrap_or(250).clamp(1, 1000);
        let order_desc = dto.order.as_deref() != Some("asc");

        let ids = search::search_metadata_ids(
            &self.pool,
            &filter,
            &SearchPage { page, size },
            order_desc,
        )
        .await?;

        let (ids, next_page) = paginate_ids(ids, size, page);
        let items = self.load_assets(auth, &ids).await?;
        Ok(empty_search_response(items, next_page))
    }

    pub async fn search_statistics(
        &self,
        auth: &AuthDto,
        dto: &StatisticsSearchReq,
    ) -> Result<SearchStatisticsResponse, ErrorResp> {
        require_permission(auth, Permission::AssetStatistics)?;
        let user_ids = self.get_user_ids(auth, None).await?;
        let mut filter = build_filter_from_base(&dto.base, user_ids);
        filter.description = dto.description.clone();
        let total = search::search_statistics_count(&self.pool, &filter).await?;
        Ok(SearchStatisticsResponse { total })
    }

    pub async fn search_random(
        &self,
        auth: &AuthDto,
        dto: &RandomSearchReq,
    ) -> Result<Vec<AssetResponse>, ErrorResp> {
        require_permission(auth, Permission::AssetRead)?;
        self.require_locked_access(auth, dto.result.base.visibility.as_deref())?;

        let user_ids = self
            .get_user_ids(auth, dto.result.base.visibility.as_deref())
            .await?;
        let filter = build_filter_from_result(&dto.result, user_ids);
        let size = dto.result.size.unwrap_or(250).clamp(1, 1000);
        let ids = search::search_random_ids(&self.pool, &filter, size).await?;
        self.load_assets(auth, &ids).await
    }

    pub async fn search_large_assets(
        &self,
        auth: &AuthDto,
        dto: &LargeAssetSearchReq,
    ) -> Result<Vec<AssetResponse>, ErrorResp> {
        require_permission(auth, Permission::AssetRead)?;
        self.require_locked_access(auth, dto.result.base.visibility.as_deref())?;

        let user_ids = self
            .get_user_ids(auth, dto.result.base.visibility.as_deref())
            .await?;
        let mut filter = build_filter_from_result(&dto.result, user_ids);
        filter.min_file_size = dto.min_file_size;
        let size = dto.result.size.unwrap_or(250).clamp(1, 1000);
        let ids = search::search_large_asset_ids(&self.pool, &filter, size).await?;
        self.load_assets(auth, &ids).await
    }

    pub async fn search_smart(
        &self,
        auth: &AuthDto,
        dto: &SmartSearchReq,
    ) -> Result<SearchResponse, ErrorResp> {
        require_permission(auth, Permission::AssetRead)?;
        self.require_locked_access(auth, dto.result.base.visibility.as_deref())?;

        let ml = get_machine_learning_config(&self.pool).await?;
        if !is_smart_search_enabled(&ml) {
            return Err(ErrorResp::BadRequest(
                "Smart search is not enabled".to_string(),
            ));
        }

        let embedding = if let Some(query) = dto.query.as_ref().filter(|q| !q.trim().is_empty()) {
            encode_clip_text(&ml, query, dto.language.as_deref()).await?
        } else if let Some(asset_id) = dto.query_asset_id {
            require_asset_access(&self.pool, auth, &asset_id, Permission::AssetRead).await?;
            search::get_smart_search_embedding(&self.pool, &asset_id)
                .await?
                .ok_or_else(|| {
                    ErrorResp::BadRequest(format!("Asset {asset_id} has no embedding"))
                })?
        } else {
            return Err(ErrorResp::BadRequest(
                "Either `query` or `queryAssetId` must be set".to_string(),
            ));
        };

        let user_ids = self
            .get_user_ids(auth, dto.result.base.visibility.as_deref())
            .await?;
        let filter = build_filter_from_result(&dto.result, user_ids);
        let page = dto.page.unwrap_or(1).max(1);
        let size = dto.result.size.unwrap_or(100).clamp(1, 1000);

        let ids = search::search_smart_ids(
            &self.pool,
            &filter,
            &embedding,
            &SearchPage { page, size },
        )
        .await?;

        let (ids, next_page) = paginate_ids(ids, size, page);
        let items = self.load_assets(auth, &ids).await?;
        Ok(empty_search_response(items, next_page))
    }

    pub async fn get_explore_data(
        &self,
        auth: &AuthDto,
    ) -> Result<Vec<SearchExploreResponse>, ErrorResp> {
        require_permission(auth, Permission::AssetRead)?;

        let city_rows =
            search::get_explore_city_asset_ids(&self.pool, &auth.user.id, 5, 12).await?;
        let city_ids: Vec<Uuid> = city_rows.iter().map(|(id, _)| *id).collect();
        let mut city_assets = self.load_assets_map(auth, &city_ids).await?;
        let city_items = city_rows
            .into_iter()
            .filter_map(|(id, city)| {
                city_assets.remove(&id).map(|data| {
                    crate::models::response::search::SearchExploreItemResponse {
                        value: city,
                        data,
                    }
                })
            })
            .collect();

        let recent_rows =
            search::get_explore_recent_asset_ids(&self.pool, &auth.user.id, 12).await?;
        let recent_ids: Vec<Uuid> = recent_rows.iter().map(|(id, _)| *id).collect();
        let mut recent_assets = self.load_assets_map(auth, &recent_ids).await?;
        let recent_items = recent_rows
            .into_iter()
            .filter_map(|(id, created_at)| {
                recent_assets.remove(&id).map(|data| {
                    crate::models::response::search::SearchExploreItemResponse {
                        value: created_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                        data,
                    }
                })
            })
            .collect();

        Ok(vec![
            SearchExploreResponse {
                field_name: "exifInfo.city".to_string(),
                items: city_items,
            },
            SearchExploreResponse {
                field_name: "createdAt".to_string(),
                items: recent_items,
            },
        ])
    }

    pub async fn search_person(
        &self,
        auth: &AuthDto,
        query: &SearchPeopleQuery,
    ) -> Result<Vec<PersonResponse>, ErrorResp> {
        require_permission(auth, Permission::PersonRead)?;
        let with_hidden = query
            .with_hidden
            .as_deref()
            .and_then(crate::utils::query::parse_query_bool)
            .unwrap_or(false);
        let rows =
            person::search_by_name(&self.pool, &auth.user.id, &query.name, with_hidden).await?;
        Ok(rows.iter().map(map_person).collect())
    }

    pub async fn search_places(
        &self,
        query: &SearchPlacesQuery,
    ) -> Result<Vec<PlacesResponse>, ErrorResp> {
        let rows = search::search_places(&self.pool, &query.name).await?;
        Ok(rows.into_iter().map(map_place).collect())
    }

    pub async fn get_assets_by_city(
        &self,
        auth: &AuthDto,
    ) -> Result<Vec<AssetResponse>, ErrorResp> {
        require_permission(auth, Permission::AssetRead)?;
        let user_ids = self.get_user_ids(auth, None).await?;
        let ids = search::get_assets_by_city_ids(&self.pool, &user_ids).await?;
        self.load_assets(auth, &ids).await
    }

    pub async fn get_search_suggestions(
        &self,
        auth: &AuthDto,
        query: &SearchSuggestionQuery,
    ) -> Result<Vec<Option<String>>, ErrorResp> {
        require_permission(auth, Permission::AssetRead)?;
        let user_ids = self.get_user_ids(auth, None).await?;

        let field = match query.suggestion_type.as_str() {
            "country" => "country",
            "state" => "state",
            "city" => "city",
            "camera-make" => "make",
            "camera-model" => "model",
            "camera-lens-model" => "lensModel",
            _ => return Ok(vec![]),
        };

        let mut suggestions = search::get_exif_suggestions(
            &self.pool,
            field,
            &user_ids,
            query.country.as_deref(),
            query.state.as_deref(),
            query.make.as_deref(),
            query.model.as_deref(),
        )
        .await?
        .into_iter()
        .map(Some)
        .collect::<Vec<_>>();

        if query
            .include_null
            .as_deref()
            .and_then(crate::utils::query::parse_query_bool)
            .unwrap_or(false)
        {
            suggestions.push(None);
        }

        Ok(suggestions)
    }

    async fn get_user_ids(
        &self,
        auth: &AuthDto,
        visibility: Option<&str>,
    ) -> Result<Vec<Uuid>, ErrorResp> {
        if visibility == Some("locked") {
            return Ok(vec![auth.user.id]);
        }
        let mut user_ids = vec![auth.user.id];
        let partners = get_timeline_partner_ids(&self.pool, &auth.user.id).await?;
        user_ids.extend(partners);
        Ok(user_ids)
    }

    fn require_locked_access(
        &self,
        auth: &AuthDto,
        visibility: Option<&str>,
    ) -> Result<(), ErrorResp> {
        if visibility == Some("locked") {
            let elevated = auth
                .session
                .as_ref()
                .is_some_and(|s| s.has_elevated_permission);
            if !elevated {
                return Err(ErrorResp::Forbidden("Forbidden".to_string()));
            }
        }
        Ok(())
    }

    async fn load_assets(
        &self,
        auth: &AuthDto,
        ids: &[Uuid],
    ) -> Result<Vec<AssetResponse>, ErrorResp> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let rows = assets::get_details_by_ids(&self.pool, ids).await?;
        let hide_exif = auth
            .shared_link
            .as_ref()
            .is_some_and(|sl| !sl.show_exif);
        let mut responses = Vec::with_capacity(rows.len());
        for row in rows {
            let stack = if let Some(stack_id) = row.stack_id {
                assets::get_stack(&self.pool, &stack_id).await?
            } else {
                None
            };
            responses.push(map_asset(&row, stack.as_ref(), auth, hide_exif));
        }
        Ok(responses)
    }

    async fn load_assets_map(
        &self,
        auth: &AuthDto,
        ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, AssetResponse>, ErrorResp> {
        let items = self.load_assets(auth, ids).await?;
        Ok(items.into_iter().map(|item| (item.id, item)).collect())
    }
}

fn build_filter_from_base(base: &BaseSearchReq, user_ids: Vec<Uuid>) -> SearchFilter {
    SearchFilter {
        user_ids,
        visibility: base.visibility.clone(),
        library_id: base.library_id,
        asset_type: base.asset_type.clone(),
        is_favorite: base.is_favorite,
        is_motion: base.is_motion,
        is_offline: base.is_offline,
        is_encoded: base.is_encoded,
        is_not_in_album: base.is_not_in_album,
        created_before: base.created_before,
        created_after: base.created_after,
        updated_before: base.updated_before,
        updated_after: base.updated_after,
        trashed_before: base.trashed_before,
        trashed_after: base.trashed_after,
        taken_before: base.taken_before,
        taken_after: base.taken_after,
        city: base.city.as_ref().map(|v| Some(v.clone())),
        state: base.state.as_ref().map(|v| Some(v.clone())),
        country: base.country.as_ref().map(|v| Some(v.clone())),
        make: base.make.as_ref().map(|v| Some(v.clone())),
        model: base.model.as_ref().map(|v| Some(v.clone())),
        lens_model: base.lens_model.as_ref().map(|v| Some(v.clone())),
        rating: base.rating.map(Some),
        ocr: base.ocr.clone(),
        person_ids: base.person_ids.clone(),
        tag_ids: base.tag_ids.clone(),
        album_ids: base.album_ids.clone(),
        ..Default::default()
    }
}

fn build_filter_from_result(dto: &ResultSearchReq, user_ids: Vec<Uuid>) -> SearchFilter {
    let mut filter = build_filter_from_base(&dto.base, user_ids);
    filter.with_deleted = dto.with_deleted.unwrap_or(false)
        || dto.base.trashed_before.is_some()
        || dto.base.trashed_after.is_some()
        || dto.base.is_offline == Some(true);
    filter.with_stacked = dto.with_stacked;
    filter
}

fn paginate_ids(ids: Vec<Uuid>, size: i64, page: i64) -> (Vec<Uuid>, Option<String>) {
    if ids.len() as i64 > size {
        (
            ids.into_iter().take(size as usize).collect(),
            Some((page + 1).to_string()),
        )
    } else {
        (ids, None)
    }
}

fn decode_checksum(value: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    if value.len() == 28 {
        base64::engine::general_purpose::STANDARD
            .decode(value)
            .ok()
    } else {
        hex::decode(value).ok()
    }
}

fn map_place(row: PlaceRow) -> PlacesResponse {
    PlacesResponse {
        name: row.name,
        latitude: row.latitude,
        longitude: row.longitude,
        admin1name: row.admin1_name,
        admin2name: row.admin2_name,
    }
}
