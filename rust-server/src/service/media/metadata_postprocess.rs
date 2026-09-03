use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::assets::{self, AssetUpdateFields, NewAsset};
use crate::models::db::face::{self, NewExifFace};
use crate::models::db::metadata_job::{self, MetadataExtractionAsset, UpsertAssetExif};
use crate::models::db::person;
use crate::models::db::system_metadata::get_json;
use crate::service::job::JobService;
use crate::service::media::exiftool::{self, tag_f64, tag_i32, tag_string, tag_value};
use crate::service::websocket::WebSocketHub;
use crate::utils::checksum::sha1_bytes;
use crate::utils::storage::StoragePaths;

const QUEUE_BACKGROUND: &str = "backgroundTask";

pub async fn run_post_processing(
    pool: &PgPool,
    jobs: &JobService,
    storage: &StoragePaths,
    websocket: &WebSocketHub,
    asset: &MetadataExtractionAsset,
    media_tags: &Value,
    exif: &UpsertAssetExif,
    file_size: i64,
    modify_date: Option<DateTime<Utc>>,
    local_date_time: Option<DateTime<Utc>>,
) -> Result<(), String> {
    if !asset
        .locked_properties
        .iter()
        .any(|property| property == "tags")
        && let Some(tags) = exif.tags.as_ref().filter(|tags| !tags.is_empty())
    {
        metadata_job::sync_asset_tags_from_exif(pool, &asset.owner_id, &asset.id, tags)
            .await
            .map_err(|err| err.to_string())?;
    }

    if is_motion_photo(asset, media_tags) {
        apply_motion_photos(
            pool,
            jobs,
            storage,
            websocket,
            asset,
            media_tags,
            exif.date_time_original,
            modify_date,
            local_date_time,
            file_size,
        )
        .await?;
    }

    if is_face_import_enabled(pool).await? && has_tagged_faces(media_tags) {
        apply_tagged_faces(pool, jobs, asset, media_tags).await?;
    }

    if exif.live_photo_cid.is_some() {
        link_live_photos(
            pool,
            websocket,
            asset,
            exif.live_photo_cid.as_deref().unwrap(),
        )
        .await?;
    }

    Ok(())
}

async fn is_face_import_enabled(pool: &PgPool) -> Result<bool, String> {
    let config = get_json(pool, "system-config")
        .await
        .map_err(|err| err.to_string())?;
    Ok(config
        .as_ref()
        .and_then(|value| value.get("metadata"))
        .and_then(|value| value.get("faces"))
        .and_then(|value| value.get("import"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false))
}

fn is_motion_photo(asset: &MetadataExtractionAsset, tags: &Value) -> bool {
    asset.asset_type == "IMAGE"
        && (tag_string(tags, "MotionPhoto").is_some() || tag_string(tags, "MicroVideo").is_some())
}

fn has_tagged_faces(tags: &Value) -> bool {
    let Some(region_info) = tag_value(tags, "RegionInfo") else {
        return false;
    };
    region_list(&region_info).is_some_and(|regions| !regions.is_empty())
        && applied_dimensions(&region_info).is_some()
}

async fn link_live_photos(
    pool: &PgPool,
    websocket: &WebSocketHub,
    asset: &MetadataExtractionAsset,
    live_photo_cid: &str,
) -> Result<(), String> {
    let other_type = if asset.asset_type == "VIDEO" {
        "IMAGE"
    } else {
        "VIDEO"
    };

    let Some(match_id) = metadata_job::find_live_photo_match(
        pool,
        &asset.owner_id,
        asset.library_id.as_ref(),
        &asset.id,
        live_photo_cid,
        other_type,
    )
    .await
    .map_err(|err| err.to_string())?
    else {
        return Ok(());
    };

    let (photo_id, motion_id) = if asset.asset_type == "IMAGE" {
        (asset.id, match_id)
    } else {
        (match_id, asset.id)
    };

    assets::update_asset_fields(
        pool,
        &photo_id,
        &AssetUpdateFields {
            is_favorite: None,
            visibility: None,
            live_photo_video_id: Some(Some(motion_id)),
            duplicate_id: None,
        },
    )
    .await
    .map_err(|err| err.to_string())?;

    assets::update_asset_fields(
        pool,
        &motion_id,
        &AssetUpdateFields {
            is_favorite: None,
            visibility: Some("hidden".to_string()),
            live_photo_video_id: None,
            duplicate_id: None,
        },
    )
    .await
    .map_err(|err| err.to_string())?;

    assets::remove_assets_from_all_albums(pool, &[motion_id])
        .await
        .map_err(|err| err.to_string())?;

    websocket.emit_asset_hidden(asset.owner_id, motion_id);

    Ok(())
}

async fn apply_motion_photos(
    pool: &PgPool,
    jobs: &JobService,
    storage: &StoragePaths,
    websocket: &WebSocketHub,
    asset: &MetadataExtractionAsset,
    tags: &Value,
    date_time_original: Option<DateTime<Utc>>,
    modify_date: Option<DateTime<Utc>>,
    local_date_time: Option<DateTime<Utc>>,
    file_size: i64,
) -> Result<(), String> {
    let is_motion_photo = tag_string(tags, "MotionPhoto").is_some();
    let is_micro_video = tag_string(tags, "MicroVideo").is_some();
    let video_offset = tag_f64(tags, "MicroVideoOffset");
    let has_motion_photo_video = tag_string(tags, "MotionPhotoVideo").is_some();
    let has_embedded_video_file = tag_string(tags, "EmbeddedVideoType")
        .is_some_and(|value| value == "MotionPhoto_Data")
        && tag_string(tags, "EmbeddedVideoFile").is_some();

    let mut length = 0usize;
    let mut padding = 0usize;

    if is_motion_photo {
        if let Some(directory_value) = tag_value(tags, "ContainerDirectory") {
            if let Some(directory) = directory_value.as_array() {
                for entry in directory {
                    if entry
                        .get("Item")
                        .and_then(|item| item.get("Semantic"))
                        .and_then(|value| value.as_str())
                        == Some("MotionPhoto")
                    {
                        length = entry
                            .get("Item")
                            .and_then(|item| item.get("Length"))
                            .and_then(|value| value.as_u64())
                            .unwrap_or(0) as usize;
                        padding = entry
                            .get("Item")
                            .and_then(|item| item.get("Padding"))
                            .and_then(|value| value.as_u64())
                            .unwrap_or(0) as usize;
                        break;
                    }
                }
            }
        }
    }

    if is_micro_video {
        if let Some(offset) = video_offset {
            length = offset.round() as usize;
        }
    }

    if length == 0 && !has_embedded_video_file && !has_motion_photo_video {
        return Ok(());
    }

    let video = if has_motion_photo_video {
        exiftool::extract_binary_tag(&asset.original_path, "MotionPhotoVideo").await?
    } else if has_embedded_video_file {
        exiftool::extract_binary_tag(&asset.original_path, "EmbeddedVideoFile").await?
    } else {
        let position = (file_size as usize)
            .saturating_sub(length)
            .saturating_sub(padding);
        tokio::fs::read(&asset.original_path)
            .await
            .map_err(|err| err.to_string())?
            .into_iter()
            .skip(position)
            .take(length)
            .collect()
    };

    if video.is_empty() {
        return Ok(());
    }

    let checksum = sha1_bytes(&video);
    let motion_asset_id =
        assets::get_by_checksum(pool, &asset.owner_id, asset.library_id.as_ref(), &checksum)
            .await
            .map_err(|err| err.to_string())?;

    let (motion_asset_id, is_new_motion_asset) = if let Some(existing_id) = motion_asset_id {
        (existing_id, false)
    } else {
        let motion_asset_id = Uuid::new_v4();
        let motion_path = storage
            .android_motion_path(&asset.owner_id, &motion_asset_id)
            .to_string_lossy()
            .into_owned();
        let motion_filename = motion_filename(&asset.original_file_name);
        let file_created_at = date_time_original
            .or(asset.file_created_at)
            .unwrap_or_else(Utc::now);
        let file_modified_at = modify_date
            .or(asset.file_modified_at)
            .unwrap_or(file_created_at);

        match assets::create_asset(
            pool,
            NewAsset {
                owner_id: asset.owner_id,
                asset_type: "VIDEO",
                original_path: &motion_path,
                checksum: &checksum,
                file_created_at,
                file_modified_at,
                is_favorite: false,
                duration: None,
                original_file_name: &motion_filename,
                live_photo_video_id: None,
                visibility: "hidden",
            },
        )
        .await
        {
            Ok(id) => {
                if !asset.is_external {
                    assets::update_quota_usage(pool, &asset.owner_id, video.len() as i64)
                        .await
                        .map_err(|err| err.to_string())?;
                }
                (id, true)
            }
            Err(_) => {
                let Some(existing_id) = assets::get_by_checksum(
                    pool,
                    &asset.owner_id,
                    asset.library_id.as_ref(),
                    &checksum,
                )
                .await
                .map_err(|err| err.to_string())?
                else {
                    return Ok(());
                };
                (existing_id, false)
            }
        }
    };

    if !is_new_motion_asset {
        let motion = assets::get_basic_by_id(pool, &motion_asset_id)
            .await
            .map_err(|err| err.to_string())?;
        if let Some(motion) = motion {
            if motion.visibility == "timeline" {
                assets::update_asset_fields(
                    pool,
                    &motion_asset_id,
                    &AssetUpdateFields {
                        is_favorite: None,
                        visibility: Some("hidden".to_string()),
                        live_photo_video_id: None,
                        duplicate_id: None,
                    },
                )
                .await
                .map_err(|err| err.to_string())?;
                websocket.emit_asset_hidden(asset.owner_id, motion_asset_id);
            }
        }
    }

    if asset.live_photo_video_id != Some(motion_asset_id) {
        assets::update_asset_fields(
            pool,
            &asset.id,
            &AssetUpdateFields {
                is_favorite: None,
                visibility: None,
                live_photo_video_id: Some(Some(motion_asset_id)),
                duplicate_id: None,
            },
        )
        .await
        .map_err(|err| err.to_string())?;

        if let Some(old_motion_id) = asset.live_photo_video_id {
            jobs.queue_json_job(
                QUEUE_BACKGROUND,
                "AssetDelete",
                serde_json::json!({
                    "id": old_motion_id,
                    "deleteOnDisk": true,
                }),
            )
            .await
            .map_err(|err| err.to_string())?;
        }
    }

    let motion = assets::get_basic_by_id(pool, &motion_asset_id)
        .await
        .map_err(|err| err.to_string())?;
    let Some(motion) = motion else {
        return Ok(());
    };

    if !Path::new(&motion.original_path).exists() {
        StoragePaths::ensure_parent(Path::new(&motion.original_path))
            .map_err(|err| err.to_string())?;
        tokio::fs::write(&motion.original_path, &video)
            .await
            .map_err(|err| err.to_string())?;

        jobs.queue_asset_extract_metadata(&motion_asset_id)
            .await
            .map_err(|err| err.to_string())?;
        jobs.queue_asset_encode_video(&motion_asset_id)
            .await
            .map_err(|err| err.to_string())?;
    }

    let _ = local_date_time;
    Ok(())
}

async fn apply_tagged_faces(
    pool: &PgPool,
    jobs: &JobService,
    asset: &MetadataExtractionAsset,
    tags: &Value,
) -> Result<(), String> {
    let Some(region_info) = tag_value(tags, "RegionInfo") else {
        return Ok(());
    };
    let Some((image_width, image_height)) =
        orient_region_info(&region_info, tag_i32(tags, "Orientation"))
    else {
        return Ok(());
    };
    let Some(regions) = orient_region_list(&region_info, tag_i32(tags, "Orientation")) else {
        return Ok(());
    };

    let existing_faces = face::get_faces_by_asset(pool, &asset.id)
        .await
        .map_err(|err| err.to_string())?;
    let faces_to_remove: Vec<Uuid> = existing_faces
        .iter()
        .filter(|face| face.source_type == "exif")
        .map(|face| face.id)
        .collect();

    let distinct_names = person::get_distinct_names(pool, &asset.owner_id)
        .await
        .map_err(|err| err.to_string())?;
    let mut name_map: std::collections::HashMap<String, Uuid> = distinct_names
        .into_iter()
        .map(|(id, name)| (name.to_lowercase(), id))
        .collect();

    let mut faces_to_add = Vec::new();
    let mut new_person_ids = Vec::new();
    let mut new_person_faces = Vec::new();

    for (name, area) in regions {
        let Some(name) = name.filter(|value| !value.is_empty()) else {
            continue;
        };
        let lowered = name.to_lowercase();
        let (person_id, is_new_person) = if let Some(existing_id) = name_map.get(&lowered) {
            (*existing_id, false)
        } else {
            let new_id = Uuid::new_v4();
            name_map.insert(lowered, new_id);
            (new_id, true)
        };

        let RegionArea { x, y, w, h } = area;
        let face_id = Uuid::new_v4();
        faces_to_add.push(NewExifFace {
            id: face_id,
            person_id,
            asset_id: asset.id,
            image_width,
            image_height,
            bounding_box_x1: ((x - w / 2.0) * image_width as f64).floor() as i32,
            bounding_box_y1: ((y - h / 2.0) * image_height as f64).floor() as i32,
            bounding_box_x2: ((x + w / 2.0) * image_width as f64).floor() as i32,
            bounding_box_y2: ((y + h / 2.0) * image_height as f64).floor() as i32,
        });

        if is_new_person {
            new_person_ids.push((person_id, name));
            new_person_faces.push((person_id, face_id));
        }
    }

    for (person_id, name) in &new_person_ids {
        person::create_with_id(pool, person_id, &asset.owner_id, name)
            .await
            .map_err(|err| err.to_string())?;
    }

    if !faces_to_remove.is_empty() || !faces_to_add.is_empty() {
        face::refresh_exif_faces(pool, &faces_to_add, &faces_to_remove)
            .await
            .map_err(|err| err.to_string())?;
    }

    for person_id in new_person_ids.iter().map(|(id, _)| id) {
        jobs.queue_person_generate_thumbnail(&asset.owner_id, person_id)
            .await
            .map_err(|err| err.to_string())?;
    }

    for (person_id, face_id) in new_person_faces {
        person::set_face_asset_id(pool, &asset.owner_id, &person_id, &face_id)
            .await
            .map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn motion_filename(original_file_name: &str) -> String {
    Path::new(original_file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| format!("{stem}.mp4"))
        .unwrap_or_else(|| "motion.mp4".to_string())
}

fn applied_dimensions(region_info: &Value) -> Option<(i32, i32)> {
    let dims = region_info.get("AppliedToDimensions")?;
    let width = dims.get("W").and_then(|v| v.as_i64())? as i32;
    let height = dims.get("H").and_then(|v| v.as_i64())? as i32;
    if width > 0 && height > 0 {
        Some((width, height))
    } else {
        None
    }
}

fn region_list(region_info: &Value) -> Option<Vec<&Value>> {
    region_info
        .get("RegionList")
        .and_then(|value| value.as_array())
        .map(|items| items.iter().collect())
}

fn region_area(region: &Value) -> Option<(f64, f64, f64, f64)> {
    let area = region.get("Area")?;
    Some((
        area.get("X").and_then(|v| v.as_f64())?,
        area.get("Y").and_then(|v| v.as_f64())?,
        area.get("W").and_then(|v| v.as_f64())?,
        area.get("H").and_then(|v| v.as_f64())?,
    ))
}

fn orient_region_info(region_info: &Value, orientation: Option<i32>) -> Option<(i32, i32)> {
    let dims = region_info.get("AppliedToDimensions")?;
    let mut width = dims.get("W").and_then(|v| v.as_i64())? as i32;
    let mut height = dims.get("H").and_then(|v| v.as_i64())? as i32;
    if width <= 0 || height <= 0 {
        return None;
    }

    let orientation = orientation.unwrap_or(1);
    if matches!(orientation, 5 | 6 | 7 | 8) {
        std::mem::swap(&mut width, &mut height);
    }
    Some((width, height))
}

fn orient_region_list(
    region_info: &Value,
    orientation: Option<i32>,
) -> Option<Vec<(Option<String>, RegionArea)>> {
    let regions = region_list(region_info)?;
    let orientation = orientation.unwrap_or(1);
    let is_sideways = matches!(orientation, 5 | 6 | 7 | 8);

    Some(
        regions
            .iter()
            .filter_map(|region| {
                let name = region
                    .get("Name")
                    .and_then(|value| value.as_str())
                    .map(str::to_string);
                let (mut x, mut y, mut w, mut h) = region_area(region)?;
                match orientation {
                    2 => x = 1.0 - x,
                    3 => {
                        x = 1.0 - x;
                        y = 1.0 - y;
                    }
                    4 => y = 1.0 - y,
                    5 => {
                        let old_x = x;
                        x = y;
                        y = old_x;
                    }
                    6 => {
                        let old_x = x;
                        x = 1.0 - y;
                        y = old_x;
                    }
                    7 => {
                        let old_x = x;
                        x = 1.0 - y;
                        y = 1.0 - old_x;
                    }
                    8 => {
                        let old_x = x;
                        x = y;
                        y = 1.0 - old_x;
                    }
                    _ => {}
                }
                if is_sideways {
                    std::mem::swap(&mut w, &mut h);
                }
                Some((name, RegionArea { x, y, w, h }))
            })
            .collect(),
    )
}

#[derive(Debug, Clone, Copy)]
struct RegionArea {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::orient_region_list;

    #[test]
    fn orients_mirrored_sideways_exif_face_regions() {
        let region_info = json!({
            "AppliedToDimensions": { "W": 100, "H": 200 },
            "RegionList": [{
                "Name": "Ada",
                "Area": { "X": 0.2, "Y": 0.3, "W": 0.1, "H": 0.2 }
            }]
        });

        let expected = [(5, 0.3, 0.2), (6, 0.7, 0.2), (7, 0.7, 0.8), (8, 0.3, 0.8)];
        for (orientation, x, y) in expected {
            let regions =
                orient_region_list(&region_info, Some(orientation)).expect("regions should parse");
            let area = regions[0].1;
            assert!((area.x - x).abs() < f64::EPSILON);
            assert!((area.y - y).abs() < f64::EPSILON);
            assert!((area.w - 0.2).abs() < f64::EPSILON);
            assert!((area.h - 0.1).abs() < f64::EPSILON);
        }
    }
}
