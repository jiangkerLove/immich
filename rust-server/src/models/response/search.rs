use serde::Serialize;
use uuid::Uuid;

use crate::models::response::asset::AssetResponse;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchAssetBucketResponse {
    pub total: i64,
    pub count: i64,
    pub items: Vec<AssetResponse>,
    pub facets: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchAlbumBucketResponse {
    pub total: i64,
    pub count: i64,
    pub items: Vec<serde_json::Value>,
    pub facets: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub albums: SearchAlbumBucketResponse,
    pub assets: SearchAssetBucketResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStatisticsResponse {
    pub total: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacesResponse {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin1name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admin2name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchExploreItemResponse {
    pub value: String,
    pub data: AssetResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchExploreResponse {
    pub field_name: String,
    pub items: Vec<SearchExploreItemResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonResponse {
    pub id: Uuid,
    pub name: String,
    pub birth_date: Option<String>,
    pub thumbnail_path: String,
    pub is_hidden: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_favorite: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

pub fn map_person(row: &crate::models::db::person::PersonRow) -> PersonResponse {
    PersonResponse {
        id: row.id,
        name: row.name.clone(),
        birth_date: row.birth_date.map(|d| d.format("%Y-%m-%d").to_string()),
        thumbnail_path: row.thumbnail_path.clone(),
        is_hidden: row.is_hidden,
        updated_at: Some(row.updated_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        is_favorite: Some(row.is_favorite),
        color: row.color.clone(),
    }
}

pub fn empty_search_response(items: Vec<AssetResponse>, next_page: Option<String>) -> SearchResponse {
    let count = items.len() as i64;
    SearchResponse {
        albums: SearchAlbumBucketResponse {
            total: 0,
            count: 0,
            items: vec![],
            facets: vec![],
        },
        assets: SearchAssetBucketResponse {
            total: count,
            count,
            items,
            facets: vec![],
            next_page,
        },
    }
}
