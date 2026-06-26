use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::models::response::asset::AssetResponse;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDataResponse {
    pub year: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryResponse {
    pub id: Uuid,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    pub memory_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_at: Option<String>,
    pub owner_id: Uuid,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub data: MemoryDataResponse,
    pub is_saved: bool,
    pub assets: Vec<AssetResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationResponse {
    pub id: Uuid,
    pub created_at: String,
    pub level: String,
    #[serde(rename = "type")]
    pub notification_type: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
}

pub fn format_datetime(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn format_optional_datetime(value: &Option<DateTime<Utc>>) -> Option<String> {
    value.as_ref().map(format_datetime)
}

pub fn parse_memory_data(data: &serde_json::Value) -> MemoryDataResponse {
    MemoryDataResponse {
        year: data
            .get("year")
            .and_then(|value| value.as_i64())
            .unwrap_or(0) as i32,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStatisticsResponse {
    pub total: i64,
}
