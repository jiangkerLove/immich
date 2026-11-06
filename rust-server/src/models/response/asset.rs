use chrono::{DateTime, Utc};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use sqlx::Pool;
use sqlx::Postgres;
use uuid::Uuid;

use crate::models::db::assets::{self, AssetDetailRow, AssetStackRow};
use crate::models::db::person;
use crate::models::db::users::avatar_color_or_default;
use crate::models::dto::auth::AuthDto;
use crate::models::response::search::{PersonResponse, map_person};
use crate::service::tag::TagResponse;
use crate::utils::mime_types;

#[derive(Debug, Clone)]
pub struct AssetUserResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub profile_image_path: String,
    pub avatar_color: String,
    pub profile_changed_at: DateTime<Utc>,
}

impl Serialize for AssetUserResponse {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AssetUserResponse", 6)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("email", &self.email)?;
        state.serialize_field("profileImagePath", &self.profile_image_path)?;
        state.serialize_field("avatarColor", &self.avatar_color)?;
        state.serialize_field("profileChangedAt", &self.profile_changed_at)?;
        state.end()
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetStackResponse {
    pub id: Uuid,
    pub primary_asset_id: Uuid,
    pub asset_count: i64,
}

#[derive(Debug, Clone)]
pub struct AssetResponse {
    pub id: Uuid,
    pub asset_type: String,
    pub thumbhash: Option<String>,
    pub original_mime_type: Option<String>,
    pub local_date_time: String,
    pub duration: Option<i32>,
    pub live_photo_video_id: Option<Uuid>,
    pub has_metadata: bool,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub created_at: String,
    pub owner_id: Uuid,
    pub owner: Option<AssetUserResponse>,
    pub library_id: Option<Uuid>,
    pub original_path: String,
    pub original_file_name: String,
    pub file_created_at: String,
    pub file_modified_at: String,
    pub updated_at: String,
    pub is_favorite: bool,
    pub is_archived: bool,
    pub is_trashed: bool,
    pub is_offline: bool,
    pub visibility: String,
    pub exif_info: Option<serde_json::Value>,
    pub tags: Option<Vec<TagResponse>>,
    pub people: Option<Vec<PersonResponse>>,
    pub checksum: String,
    pub stack: Option<AssetStackResponse>,
    pub duplicate_id: Option<Uuid>,
    pub resized: bool,
    pub is_edited: bool,
}

impl Serialize for AssetResponse {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Matches SanitizedAssetResponseDto when stripMetadata is set.
        if !self.has_metadata {
            let mut state = serializer.serialize_struct("AssetResponse", 10)?;
            state.serialize_field("id", &self.id)?;
            state.serialize_field("type", &self.asset_type)?;
            state.serialize_field("thumbhash", &self.thumbhash)?;
            state.serialize_field("originalMimeType", &self.original_mime_type)?;
            state.serialize_field("localDateTime", &self.local_date_time)?;
            state.serialize_field("duration", &self.duration)?;
            state.serialize_field("livePhotoVideoId", &self.live_photo_video_id)?;
            state.serialize_field("hasMetadata", &false)?;
            state.serialize_field("width", &self.width)?;
            state.serialize_field("height", &self.height)?;
            return state.end();
        }

        let mut state = serializer.serialize_struct("AssetResponse", 28)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("type", &self.asset_type)?;
        state.serialize_field("thumbhash", &self.thumbhash)?;
        state.serialize_field("originalMimeType", &self.original_mime_type)?;
        state.serialize_field("localDateTime", &self.local_date_time)?;
        state.serialize_field("duration", &self.duration)?;
        state.serialize_field("livePhotoVideoId", &self.live_photo_video_id)?;
        state.serialize_field("hasMetadata", &true)?;
        state.serialize_field("width", &self.width)?;
        state.serialize_field("height", &self.height)?;
        state.serialize_field("createdAt", &self.created_at)?;
        state.serialize_field("ownerId", &self.owner_id)?;
        if let Some(owner) = &self.owner {
            state.serialize_field("owner", owner)?;
        }
        state.serialize_field("libraryId", &self.library_id)?;
        state.serialize_field("originalPath", &self.original_path)?;
        state.serialize_field("originalFileName", &self.original_file_name)?;
        state.serialize_field("fileCreatedAt", &self.file_created_at)?;
        state.serialize_field("fileModifiedAt", &self.file_modified_at)?;
        state.serialize_field("updatedAt", &self.updated_at)?;
        state.serialize_field("isFavorite", &self.is_favorite)?;
        state.serialize_field("isArchived", &self.is_archived)?;
        state.serialize_field("isTrashed", &self.is_trashed)?;
        state.serialize_field("isOffline", &self.is_offline)?;
        state.serialize_field("visibility", &self.visibility)?;
        if let Some(exif) = &self.exif_info {
            state.serialize_field("exifInfo", exif)?;
        }
        if let Some(tags) = &self.tags {
            state.serialize_field("tags", tags)?;
        }
        if let Some(people) = &self.people {
            state.serialize_field("people", people)?;
        }
        state.serialize_field("checksum", &self.checksum)?;
        if let Some(stack) = &self.stack {
            state.serialize_field("stack", stack)?;
        }
        state.serialize_field("duplicateId", &self.duplicate_id)?;
        state.serialize_field("resized", &self.resized)?;
        state.serialize_field("isEdited", &self.is_edited)?;
        state.end()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetStatsResponse {
    pub images: i64,
    pub videos: i64,
    pub total: i64,
}

pub fn map_asset(
    row: &AssetDetailRow,
    stack: Option<&AssetStackRow>,
    auth: &AuthDto,
    strip_metadata: bool,
    people: Option<&[PersonResponse]>,
) -> AssetResponse {
    let original_mime_type = Some(mime_types::lookup(&row.original_file_name));

    if strip_metadata {
        return AssetResponse {
            id: row.id,
            asset_type: row.asset_type.clone(),
            thumbhash: encode_optional_base64(row.thumbhash.as_deref()),
            original_mime_type,
            local_date_time: format_datetime(&row.local_date_time),
            duration: row.duration,
            live_photo_video_id: row.live_photo_video_id,
            has_metadata: false,
            width: row.width,
            height: row.height,
            created_at: String::new(),
            owner_id: Uuid::nil(),
            owner: None,
            library_id: None,
            original_path: String::new(),
            original_file_name: String::new(),
            file_created_at: String::new(),
            file_modified_at: String::new(),
            updated_at: String::new(),
            is_favorite: false,
            is_archived: false,
            is_trashed: false,
            is_offline: false,
            visibility: String::new(),
            exif_info: None,
            tags: None,
            people: None,
            checksum: String::new(),
            stack: None,
            duplicate_id: None,
            resized: false,
            is_edited: false,
        };
    }

    let is_favorite = auth.user.id == row.owner_id && row.is_favorite;
    let include_owner = auth.shared_link.is_none();

    AssetResponse {
        id: row.id,
        asset_type: row.asset_type.clone(),
        thumbhash: encode_optional_base64(row.thumbhash.as_deref()),
        original_mime_type,
        local_date_time: format_datetime(&row.local_date_time),
        duration: row.duration,
        live_photo_video_id: row.live_photo_video_id,
        has_metadata: true,
        width: row.width,
        height: row.height,
        created_at: format_datetime(&row.created_at),
        owner_id: row.owner_id,
        owner: include_owner.then(|| AssetUserResponse {
            id: row.owner_id,
            name: row.owner_name.clone(),
            email: row.owner_email.clone(),
            profile_image_path: row.owner_profile_image_path.clone(),
            avatar_color: avatar_color_or_default(
                &row.owner_email,
                row.owner_avatar_color.as_deref(),
            ),
            profile_changed_at: row.owner_profile_changed_at,
        }),
        library_id: row.library_id,
        original_path: row.original_path.clone(),
        original_file_name: row.original_file_name.clone(),
        file_created_at: format_datetime(&row.file_created_at),
        file_modified_at: format_datetime(&row.file_modified_at),
        updated_at: format_datetime(&row.updated_at),
        is_favorite,
        is_archived: row.visibility == "archive",
        is_trashed: row.deleted_at.is_some(),
        is_offline: row.is_offline,
        visibility: row.visibility.clone(),
        exif_info: row.exif_json.as_ref().map(map_exif),
        tags: row.tags_json.as_ref().map(parse_tags),
        people: if auth.shared_link.is_some() {
            Some(vec![])
        } else {
            Some(people.unwrap_or(&[]).to_vec())
        },
        checksum: base64_encode(&row.checksum),
        stack: stack.map(map_stack),
        duplicate_id: row.duplicate_id,
        resized: true,
        is_edited: row.is_edited,
    }
}

fn map_stack(stack: &AssetStackRow) -> AssetStackResponse {
    AssetStackResponse {
        id: stack.id,
        primary_asset_id: stack.primary_asset_id,
        asset_count: stack.asset_count,
    }
}

/// Matches `mapExif` in `server/src/dtos/exif.dto.ts`.
fn map_exif(exif: &serde_json::Value) -> serde_json::Value {
    let obj = match exif.as_object() {
        Some(obj) => obj,
        None => return serde_json::Value::Null,
    };

    let file_size = obj.get("fileSizeInByte").and_then(|value| {
        if value.as_i64() == Some(0) || value.as_u64() == Some(0) || value.is_null() {
            None
        } else if let Some(n) = value.as_i64() {
            Some(serde_json::json!(n))
        } else if let Some(n) = value.as_u64() {
            Some(serde_json::json!(n))
        } else if let Some(text) = value.as_str() {
            text.parse::<i64>()
                .ok()
                .filter(|n| *n != 0)
                .map(|n| serde_json::json!(n))
        } else {
            None
        }
    });

    serde_json::json!({
        "make": obj.get("make").cloned().unwrap_or(serde_json::Value::Null),
        "model": obj.get("model").cloned().unwrap_or(serde_json::Value::Null),
        "exifImageWidth": obj.get("exifImageWidth").cloned().unwrap_or(serde_json::Value::Null),
        "exifImageHeight": obj.get("exifImageHeight").cloned().unwrap_or(serde_json::Value::Null),
        "fileSizeInByte": file_size.unwrap_or(serde_json::Value::Null),
        "orientation": obj.get("orientation").cloned().unwrap_or(serde_json::Value::Null),
        "dateTimeOriginal": format_exif_datetime(obj.get("dateTimeOriginal")),
        "modifyDate": format_exif_datetime(obj.get("modifyDate")),
        "timeZone": obj.get("timeZone").cloned().unwrap_or(serde_json::Value::Null),
        "lensModel": obj.get("lensModel").cloned().unwrap_or(serde_json::Value::Null),
        "fNumber": obj.get("fNumber").cloned().unwrap_or(serde_json::Value::Null),
        "focalLength": obj.get("focalLength").cloned().unwrap_or(serde_json::Value::Null),
        "iso": obj.get("iso").cloned().unwrap_or(serde_json::Value::Null),
        "exposureTime": obj.get("exposureTime").cloned().unwrap_or(serde_json::Value::Null),
        "latitude": obj.get("latitude").cloned().unwrap_or(serde_json::Value::Null),
        "longitude": obj.get("longitude").cloned().unwrap_or(serde_json::Value::Null),
        "city": obj.get("city").cloned().unwrap_or(serde_json::Value::Null),
        "state": obj.get("state").cloned().unwrap_or(serde_json::Value::Null),
        "country": obj.get("country").cloned().unwrap_or(serde_json::Value::Null),
        "description": obj.get("description").cloned().unwrap_or(serde_json::Value::Null),
        "projectionType": obj.get("projectionType").cloned().unwrap_or(serde_json::Value::Null),
        "rating": obj.get("rating").cloned().unwrap_or(serde_json::Value::Null),
    })
}

fn format_exif_datetime(value: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(value) = value else {
        return serde_json::Value::Null;
    };
    if value.is_null() {
        return serde_json::Value::Null;
    }
    if let Some(text) = value.as_str() {
        if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
            return serde_json::Value::String(
                dt.with_timezone(&Utc)
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            );
        }
        return serde_json::Value::String(text.to_string());
    }
    value.clone()
}

fn parse_tags(value: &serde_json::Value) -> Vec<TagResponse> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(TagResponse {
                        id: Uuid::parse_str(item.get("id")?.as_str()?).ok()?,
                        value: item.get("value")?.as_str()?.to_string(),
                        created_at: item.get("createdAt")?.as_str()?.parse().ok()?,
                        updated_at: item.get("updatedAt")?.as_str()?.parse().ok()?,
                        color: item
                            .get("color")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        parent_id: item
                            .get("parentId")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok()),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn format_datetime(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn encode_optional_base64(bytes: Option<&[u8]>) -> Option<String> {
    bytes.map(base64_encode)
}

pub async fn map_folder_assets(
    _pool: &Pool<Postgres>,
    rows: &[AssetDetailRow],
    auth: &AuthDto,
) -> Result<Vec<AssetResponse>, sqlx::Error> {
    Ok(rows
        .iter()
        .map(|row| {
            let mut asset = map_asset(row, None, auth, false, None);
            asset.owner = None;
            asset.people = None;
            asset.stack = None;
            asset.tags = None;
            asset
        })
        .collect())
}

pub async fn map_assets(
    pool: &Pool<Postgres>,
    rows: &[AssetDetailRow],
    auth: &AuthDto,
    strip_metadata: bool,
    with_stack: bool,
) -> Result<Vec<AssetResponse>, sqlx::Error> {
    if rows.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
    let people_map = if strip_metadata || auth.shared_link.is_some() {
        std::collections::HashMap::new()
    } else {
        person::get_people_by_asset_ids(pool, &ids, &auth.user.id)
            .await?
            .into_iter()
            .map(|(asset_id, person_rows)| {
                (
                    asset_id,
                    person_rows.iter().map(map_person).collect::<Vec<_>>(),
                )
            })
            .collect()
    };

    let mut responses = Vec::with_capacity(rows.len());
    for row in rows {
        let stack = if with_stack && !strip_metadata {
            if let Some(stack_id) = row.stack_id {
                assets::get_stack(pool, &stack_id).await?
            } else {
                None
            }
        } else {
            None
        };
        let people = people_map.get(&row.id).map(|items| items.as_slice());
        responses.push(map_asset(row, stack.as_ref(), auth, strip_metadata, people));
    }
    Ok(responses)
}

pub async fn get_asset_response(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<AssetResponse>, sqlx::Error> {
    let Some(row) = assets::get_detail_by_id(pool, asset_id).await? else {
        return Ok(None);
    };
    let auth = AuthDto {
        user: crate::models::db::users::AuthUserDb {
            id: row.owner_id,
            is_admin: false,
            name: row.owner_name.clone(),
            email: row.owner_email.clone(),
            quota_usage_in_bytes: 0,
            quota_size_in_bytes: None,
        },
        api_key: None,
        session: None,
        shared_link: None,
    };
    Ok(
        map_assets(pool, std::slice::from_ref(&row), &auth, false, true)
            .await?
            .into_iter()
            .next(),
    )
}

#[cfg(test)]
mod tests {
    use super::map_exif;
    use serde_json::json;

    #[test]
    fn map_exif_drops_extra_columns_and_zero_file_size() {
        let mapped = map_exif(&json!({
            "make": "Canon",
            "assetId": "x",
            "fileSizeInByte": 0,
            "bitsPerSample": 12,
            "dateTimeOriginal": "2023-11-19T18:11:00+00:00",
        }));
        assert_eq!(mapped["make"], "Canon");
        assert!(mapped.get("assetId").is_none() || mapped.get("assetId").is_none());
        assert!(!mapped.as_object().unwrap().contains_key("assetId"));
        assert!(!mapped.as_object().unwrap().contains_key("bitsPerSample"));
        assert!(mapped["fileSizeInByte"].is_null());
        assert_eq!(mapped["dateTimeOriginal"], "2023-11-19T18:11:00.000Z");
    }
}
