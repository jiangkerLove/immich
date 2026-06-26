use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::models::db::assets::AssetDetailRow;
use crate::utils::bytes::hex_or_buffer_to_base64;

fn format_dt(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn opt_dt(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|v| format_dt(&v))
}

fn opt_uuid(value: Option<Uuid>) -> Option<String> {
    value.map(|v| v.to_string())
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncAssetV2 {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub original_file_name: String,
    pub thumbhash: Option<String>,
    pub checksum: String,
    pub file_created_at: String,
    pub file_modified_at: String,
    pub created_at: String,
    pub local_date_time: String,
    pub duration: Option<i32>,
    #[serde(rename = "type")]
    pub asset_type: String,
    pub deleted_at: Option<String>,
    pub is_favorite: bool,
    pub visibility: String,
    pub live_photo_video_id: Option<String>,
    pub stack_id: Option<String>,
    pub library_id: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub is_edited: bool,
}

impl From<&AssetDetailRow> for SyncAssetV2 {
    fn from(row: &AssetDetailRow) -> Self {
        Self {
            id: row.id,
            owner_id: row.owner_id,
            original_file_name: row.original_file_name.clone(),
            thumbhash: row
                .thumbhash
                .as_deref()
                .map(hex_or_buffer_to_base64),
            checksum: hex_or_buffer_to_base64(&row.checksum),
            file_created_at: format_dt(&row.file_created_at),
            file_modified_at: format_dt(&row.file_modified_at),
            created_at: format_dt(&row.created_at),
            local_date_time: format_dt(&row.local_date_time),
            duration: row.duration,
            asset_type: row.asset_type.clone(),
            deleted_at: opt_dt(row.deleted_at),
            is_favorite: row.is_favorite,
            visibility: row.visibility.clone(),
            live_photo_video_id: opt_uuid(row.live_photo_video_id),
            stack_id: opt_uuid(row.stack_id),
            library_id: opt_uuid(row.library_id),
            width: row.width,
            height: row.height,
            is_edited: row.is_edited,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncAssetExifV1 {
    pub asset_id: Uuid,
    pub description: Option<String>,
    pub exif_image_width: Option<i32>,
    pub exif_image_height: Option<i32>,
    pub file_size_in_byte: Option<i64>,
    pub orientation: Option<String>,
    pub date_time_original: Option<String>,
    pub modify_date: Option<String>,
    pub time_zone: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub projection_type: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens_model: Option<String>,
    pub f_number: Option<f64>,
    pub focal_length: Option<f64>,
    pub iso: Option<i32>,
    pub exposure_time: Option<String>,
    pub profile_description: Option<String>,
    pub rating: Option<i32>,
    pub fps: Option<f64>,
}

pub fn sync_exif_from_json(asset_id: Uuid, exif: &Value) -> SyncAssetExifV1 {
    let get_str = |key: &str| exif.get(key).and_then(|v| v.as_str()).map(str::to_string);
    let get_i32 = |key: &str| exif.get(key).and_then(|v| v.as_i64()).map(|v| v as i32);
    let get_i64 = |key: &str| exif.get(key).and_then(|v| v.as_i64());
    let get_f64 = |key: &str| exif.get(key).and_then(|v| v.as_f64());

    SyncAssetExifV1 {
        asset_id,
        description: get_str("description"),
        exif_image_width: get_i32("exifImageWidth"),
        exif_image_height: get_i32("exifImageHeight"),
        file_size_in_byte: get_i64("fileSizeInByte"),
        orientation: get_str("orientation"),
        date_time_original: exif
            .get("dateTimeOriginal")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        modify_date: exif
            .get("modifyDate")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        time_zone: get_str("timeZone"),
        latitude: get_f64("latitude"),
        longitude: get_f64("longitude"),
        projection_type: get_str("projectionType"),
        city: get_str("city"),
        state: get_str("state"),
        country: get_str("country"),
        make: get_str("make"),
        model: get_str("model"),
        lens_model: get_str("lensModel"),
        f_number: get_f64("fNumber"),
        focal_length: get_f64("focalLength"),
        iso: get_i32("iso"),
        exposure_time: get_str("exposureTime"),
        profile_description: get_str("profileDescription"),
        rating: get_i32("rating"),
        fps: get_f64("fps"),
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncAssetEditV1 {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub action: String,
    pub parameters: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetUploadReadyV2 {
    pub asset: SyncAssetV2,
    pub exif: SyncAssetExifV1,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEditReadyV2 {
    pub asset: SyncAssetV2,
    pub edit: Vec<SyncAssetEditV1>,
}
