use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Pool;
use sqlx::Postgres;
use uuid::Uuid;

use crate::models::db::assets::{self, AssetDetailRow, AssetStackRow};
use crate::models::db::person;
use crate::models::dto::auth::AuthDto;
use crate::models::response::search::{map_person, PersonResponse};
use crate::service::tag::TagResponse;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetUserResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub profile_image_path: String,
    pub avatar_color: String,
    pub profile_changed_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetStackResponse {
    pub id: Uuid,
    pub primary_asset_id: Uuid,
    pub asset_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExifResponse {
    #[serde(flatten)]
    pub fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetResponse {
    pub id: Uuid,
    #[serde(rename = "type")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exif_info: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<TagResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub people: Option<Vec<PersonResponse>>,
    pub checksum: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<AssetStackResponse>,
    pub duplicate_id: Option<Uuid>,
    pub resized: bool,
    pub is_edited: bool,
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
    if strip_metadata {
        return AssetResponse {
            id: row.id,
            asset_type: row.asset_type.clone(),
            thumbhash: encode_optional_base64(row.thumbhash.as_deref()),
            original_mime_type: mime_guess::from_path(&row.original_file_name)
                .first()
                .map(|m| m.essence_str().to_string()),
            local_date_time: format_datetime(&row.local_date_time),
            duration: row.duration,
            live_photo_video_id: row.live_photo_video_id,
            has_metadata: false,
            width: row.width,
            height: row.height,
            created_at: format_datetime(&row.created_at),
            owner_id: row.owner_id,
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
            visibility: row.visibility.clone(),
            exif_info: None,
            tags: None,
            people: None,
            checksum: String::new(),
            stack: stack.map(map_stack),
            duplicate_id: None,
            resized: true,
            is_edited: row.is_edited,
        };
    }

    let is_favorite = auth.user.id == row.owner_id && row.is_favorite;
    let include_owner = auth.shared_link.is_none();

    AssetResponse {
        id: row.id,
        asset_type: row.asset_type.clone(),
        thumbhash: encode_optional_base64(row.thumbhash.as_deref()),
        original_mime_type: mime_guess::from_path(&row.original_file_name)
            .first()
            .map(|m| m.essence_str().to_string()),
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
            avatar_color: row
                .owner_avatar_color
                .clone()
                .unwrap_or_else(|| "primary".to_string()),
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
        exif_info: row.exif_json.clone(),
        tags: row.tags_json.as_ref().map(parse_tags),
        people: Some(people.unwrap_or(&[]).to_vec()),
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
                        created_at: item
                            .get("createdAt")?
                            .as_str()?
                            .parse()
                            .ok()?,
                        updated_at: item
                            .get("updatedAt")?
                            .as_str()?
                            .parse()
                            .ok()?,
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

pub async fn map_assets(
    pool: &Pool<Postgres>,
    rows: &[AssetDetailRow],
    auth: &AuthDto,
    strip_metadata: bool,
) -> Result<Vec<AssetResponse>, sqlx::Error> {
    if rows.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
    let people_map = if strip_metadata {
        std::collections::HashMap::new()
    } else {
        person::get_people_by_asset_ids(pool, &ids)
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
        let stack = if let Some(stack_id) = row.stack_id {
            assets::get_stack(pool, &stack_id).await?
        } else {
            None
        };
        let people = people_map.get(&row.id).map(|items| items.as_slice());
        responses.push(map_asset(
            row,
            stack.as_ref(),
            auth,
            strip_metadata,
            people,
        ));
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
    Ok(map_assets(pool, std::slice::from_ref(&row), &auth, false)
        .await?
        .into_iter()
        .next())
}
