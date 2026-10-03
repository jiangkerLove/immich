use std::path::Path;

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::map;
use crate::models::db::metadata_job::{
    self, UpdateAssetAfterMetadata, UpsertAssetAudio, UpsertAssetExif, UpsertAssetKeyframe,
    UpsertAssetVideo,
};
use crate::service::job::EntityJob;
use crate::service::job::JobService;
use crate::service::media::exiftool::{
    self, tag_f64, tag_i32, tag_string, tag_validated_f64, tag_validated_i32, tag_value,
};
use crate::service::media::ffprobe::{self, ProbeResult};
use crate::service::media::metadata_postprocess;
use crate::service::websocket::WebSocketHub;
use crate::utils::mime_types::{is_heif_image_path, is_possibly_animated_image_path};
use crate::utils::storage::StoragePaths;
use crate::utils::system_config::get_merged;

const JOBS_BATCH_SIZE: usize = 1000;
const EXIF_DATE_TAGS: &[&str] = &[
    "SubSecDateTimeOriginal",
    "SubSecCreateDate",
    "DateTimeOriginal",
    "CreationDate",
    "CreateDate",
    "MediaCreateDate",
    "DateTimeCreated",
    "GPSDateTime",
    "DateTimeUTC",
    "SonyDateTime2",
    "SourceImageCreateTime",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataExtractOutcome {
    Success,
    NotFound,
    Failed,
}

#[derive(Clone)]
pub struct MetadataExtractService {
    pool: PgPool,
    jobs: JobService,
    storage: StoragePaths,
    websocket: WebSocketHub,
}

impl MetadataExtractService {
    pub fn new(
        pool: PgPool,
        storage: StoragePaths,
        jobs: JobService,
        websocket: WebSocketHub,
    ) -> Self {
        Self {
            pool,
            jobs,
            storage,
            websocket,
        }
    }

    pub async fn extract_asset_metadata(
        &self,
        asset_id: &Uuid,
        job: &EntityJob,
    ) -> Result<MetadataExtractOutcome, String> {
        let Some(asset) = metadata_job::get_for_metadata_extraction(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(MetadataExtractOutcome::NotFound);
        };

        if !Path::new(&asset.original_path).exists() {
            return Ok(MetadataExtractOutcome::Failed);
        }

        let should_probe = asset.asset_type == "VIDEO"
            || asset.original_path.to_ascii_lowercase().ends_with(".gif");

        let mut media_tags = exiftool::read_tags(&asset.original_path, should_probe).await?;
        if let Some(sidecar) = asset.sidecar_path.as_ref() {
            if Path::new(sidecar).exists() {
                if let Ok(sidecar_tags) = exiftool::read_tags(sidecar, false).await {
                    merge_sidecar_tags(&mut media_tags, &sidecar_tags);
                }
            }
        }
        let probe = if should_probe {
            Some(ffprobe::probe(&asset.original_path).await?)
        } else {
            None
        };

        // Official deletes the EXIF Duration tag when a video probe ran, or when the
        // file cannot be an animation (for example a CR3 that reports Duration: 1s).
        if probe.is_some() || !is_possibly_animated_image_path(&asset.original_path) {
            remove_tag(&mut media_tags, "Duration");
        }

        if let Some(probe) = probe.as_ref() {
            merge_probe_tags(&mut media_tags, probe);
        }
        apply_heif_orientation(&mut media_tags, &asset.original_path);

        let metadata = tokio::fs::metadata(&asset.original_path)
            .await
            .map_err(|err| err.to_string())?;
        let modify_date = metadata.modified().ok().and_then(system_time_to_utc);
        let created_date = metadata.created().ok().and_then(system_time_to_utc);
        let fallback_date = fallback_asset_date(asset.file_created_at, created_date, modify_date);
        let file_size = metadata.len() as i64;

        let (width, height) = image_dimensions(&media_tags);
        let orientation = tag_validated_i32(&media_tags, "Orientation").map(|v| v.to_string());
        let is_sideways = orientation
            .as_deref()
            .is_some_and(|v| matches!(v, "5" | "6" | "7" | "8" | "90" | "-90"));
        let asset_width = if is_sideways { height } else { width };
        let asset_height = if is_sideways { width } else { height };

        let (latitude, longitude) = gps_coordinates(&media_tags);
        let (city, state, country) = self.reverse_geocode(latitude, longitude).await?;

        let tags = collect_tags(&media_tags);
        let raw_exif_date = EXIF_DATE_TAGS
            .iter()
            .find_map(|tag| tag_string(&media_tags, tag));
        let time_zone =
            resolve_time_zone(&media_tags, raw_exif_date.as_deref(), latitude, longitude);
        let exif_date = raw_exif_date
            .as_deref()
            .and_then(|raw| parse_exif_date(raw, time_zone.as_deref()));
        let date_time_original = exif_date
            .as_ref()
            .map(|date| date.instant)
            .or(fallback_date);
        let time_zone =
            time_zone.or_else(|| exif_date.as_ref().and_then(|date| date.time_zone.clone()));
        let exif = UpsertAssetExif {
            asset_id: asset.id,
            make: camera_make(&media_tags),
            model: camera_model(&media_tags),
            exif_image_width: width,
            exif_image_height: height,
            file_size_in_byte: Some(file_size),
            orientation,
            date_time_original,
            modify_date,
            lens_model: lens_model(&media_tags),
            f_number: tag_validated_f64(&media_tags, "FNumber"),
            focal_length: tag_validated_f64(&media_tags, "FocalLength"),
            iso: tag_validated_i32(&media_tags, "ISO"),
            latitude,
            longitude,
            city,
            state,
            country,
            description: tag_string(&media_tags, "ImageDescription")
                .or_else(|| tag_string(&media_tags, "Description"))
                .unwrap_or_default()
                .trim()
                .to_string(),
            fps: probe
                .as_ref()
                .and_then(|p| p.video.as_ref())
                .and_then(|v| v.frame_rate)
                .or_else(|| tag_f64(&media_tags, "VideoFrameRate")),
            exposure_time: tag_string(&media_tags, "ExposureTime"),
            live_photo_cid: tag_string(&media_tags, "ContentIdentifier")
                .or_else(|| tag_string(&media_tags, "MediaGroupUUID")),
            time_zone,
            projection_type: tag_string(&media_tags, "ProjectionType")
                .map(|v| v.to_ascii_uppercase()),
            profile_description: tag_string(&media_tags, "ProfileDescription"),
            colorspace: tag_string(&media_tags, "ColorSpace"),
            bits_per_sample: bits_per_sample(&media_tags),
            auto_stack_id: tag_string(&media_tags, "BurstID")
                .or_else(|| tag_string(&media_tags, "BurstUUID"))
                .or_else(|| tag_string(&media_tags, "CameraBurstID"))
                .or_else(|| tag_string(&media_tags, "MediaUniqueID")),
            rating: tag_validated_i32(&media_tags, "Rating").filter(|v| (1..=5).contains(v)),
            tags: if tags.is_empty() { None } else { Some(tags) },
        };

        let video = probe.as_ref().and_then(|probe| {
            probe.video.as_ref().map(|video| UpsertAssetVideo {
                asset_id: asset.id,
                bitrate: video.bitrate.clamp(0, i64::from(i32::MAX)) as i32,
                frame_count: video.frame_count.clamp(0, i64::from(i32::MAX)) as i32,
                time_base: video.time_base_den,
                index: video.index as i16,
                profile: video.profile.map(|v| v as i16),
                level: video.level.map(|v| v as i16),
                color_primaries: video.color_primaries,
                color_transfer: video.color_transfer,
                color_matrix: video.color_matrix,
                dv_profile: video.dv_profile,
                dv_level: video.dv_level,
                dv_bl_signal_compatibility_id: video.dv_bl_signal_compatibility_id,
                codec_name: video.codec_name.clone(),
                format_name: probe.format.format_name.clone(),
                format_long_name: probe.format.format_long_name.clone(),
                pixel_format: video.pixel_format.clone(),
            })
        });

        let audio = probe
            .as_ref()
            .and_then(|p| p.audio.as_ref())
            .map(|audio| UpsertAssetAudio {
                asset_id: asset.id,
                bitrate: audio.bitrate.clamp(0, i64::from(i32::MAX)) as i32,
                index: audio.index as i16,
                profile: audio.profile.map(|v| v as i16),
                codec_name: audio.codec_name.clone(),
            });

        let keyframe = if let Some(video) = probe.as_ref().and_then(|probe| probe.video.as_ref()) {
            ffprobe::probe_packets(&asset.original_path, video.index)
                .await?
                .filter(|packets| !packets.keyframe_pts.is_empty())
                .map(|packets| UpsertAssetKeyframe {
                    asset_id: asset.id,
                    pts: packets.keyframe_pts,
                    acc_duration: packets.keyframe_acc_duration,
                    own_duration: packets.keyframe_own_duration,
                    total_duration: packets.total_duration,
                    packet_count: packets.packet_count,
                    output_frames: packets.output_frames,
                })
        } else {
            None
        };

        let duration_ms = probe
            .as_ref()
            .and_then(|p| p.format.duration)
            .map(|seconds| (seconds * 1000.0).round() as i64)
            .or_else(|| tag_f64(&media_tags, "Duration").map(|s| (s * 1000.0).round() as i64));

        let local_date_time = exif_date.as_ref().map(|date| date.local).or(fallback_date);

        let (update_width, update_height) = metadata_dimension_updates(
            asset.is_edited,
            asset.width,
            asset.height,
            asset_width,
            asset_height,
        );

        metadata_job::upsert_metadata(
            &self.pool,
            &exif,
            video.as_ref(),
            audio.as_ref(),
            keyframe.as_ref(),
            &UpdateAssetAfterMetadata {
                asset_id: asset.id,
                duration: duration_ms,
                local_date_time,
                file_created_at: exif.date_time_original.or(fallback_date),
                file_modified_at: modify_date.or(asset.file_modified_at),
                width: update_width,
                height: update_height,
            },
        )
        .await
        .map_err(|err| err.to_string())?;

        metadata_postprocess::run_post_processing(
            &self.pool,
            &self.jobs,
            &self.storage,
            &self.websocket,
            &asset,
            &media_tags,
            &exif,
            file_size,
            modify_date,
            local_date_time,
        )
        .await?;

        self.queue_follow_up_jobs(job).await?;

        if job.source.as_deref() != Some("sidecar-write") {
            let _ = crate::service::workflow_trigger::on_asset_trigger(
                &self.pool,
                &self.jobs,
                &asset.owner_id,
                asset_id,
                crate::utils::workflow::TRIGGER_ASSET_METADATA,
            )
            .await;
        }

        Ok(MetadataExtractOutcome::Success)
    }

    pub async fn queue_all_metadata_extraction(&self, force: bool) -> Result<(), String> {
        let asset_ids = metadata_job::stream_for_metadata_extraction(&self.pool, force)
            .await
            .map_err(|err| err.to_string())?;

        for chunk in asset_ids.chunks(JOBS_BATCH_SIZE) {
            for asset_id in chunk {
                self.jobs
                    .queue_asset_extract_metadata(asset_id)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }
        Ok(())
    }

    async fn reverse_geocode(
        &self,
        latitude: Option<f64>,
        longitude: Option<f64>,
    ) -> Result<(Option<String>, Option<String>, Option<String>), String> {
        let (Some(lat), Some(lon)) = (latitude, longitude) else {
            return Ok((None, None, None));
        };

        let config = get_merged(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        let enabled = config
            .get("reverseGeocoding")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !enabled {
            return Ok((None, None, None));
        }

        let place = map::reverse_geocode_places(&self.pool, lat, lon)
            .await
            .map_err(|err| err.to_string())?;
        Ok((
            place.as_ref().and_then(|p| p.city.clone()),
            place.as_ref().and_then(|p| p.state.clone()),
            place.as_ref().and_then(|p| p.country_code.clone()),
        ))
    }

    async fn queue_follow_up_jobs(&self, job: &EntityJob) -> Result<(), String> {
        let config = get_merged(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        let template_enabled = config
            .get("storageTemplate")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        if template_enabled {
            self.jobs
                .queue_storage_template_migration_single(&job.id, job.source.as_deref())
                .await
                .map_err(|err| err.to_string())?;
        } else if matches!(job.source.as_deref(), Some("upload") | Some("copy")) {
            self.jobs
                .queue_asset_generate_thumbnails_with_notify(&job.id, job.notify.unwrap_or(false))
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(())
    }
}

fn merge_sidecar_tags(media: &mut Value, sidecar: &Value) {
    if EXIF_DATE_TAGS
        .iter()
        .any(|tag| tag_string(sidecar, tag).is_some())
    {
        for tag in EXIF_DATE_TAGS {
            remove_tag(media, tag);
        }
        remove_tag(media, "OffsetTime");
        remove_tag(media, "TimeZone");
    }

    if let Some(obj) = sidecar.as_object() {
        for (key, value) in obj {
            if key == "SourceFile" || key == "ExifToolVersion" || key == "Duration" {
                continue;
            }
            if let Some(map) = media.as_object_mut() {
                map.insert(key.clone(), value.clone());
            }
        }
    }
}

fn apply_heif_orientation(media: &mut Value, path: &str) {
    if !is_heif_image_path(path) {
        return;
    }

    let orientation = tag_i32(media, "Rotation").and_then(|rotation| match rotation {
        0 => Some(1),
        1 => Some(8),
        2 => Some(3),
        3 => Some(6),
        _ => None,
    });

    match orientation {
        Some(orientation) => set_tag(media, "Orientation", Value::from(orientation)),
        None => remove_tag(media, "Orientation"),
    }
}

fn merge_probe_tags(media: &mut Value, probe: &ProbeResult) {
    if let Some(video) = probe.video.as_ref() {
        if video.width > 0 {
            set_tag(media, "ImageWidth", Value::from(video.width));
        }
        if video.height > 0 {
            set_tag(media, "ImageHeight", Value::from(video.height));
        }
        set_tag(
            media,
            "Orientation",
            orientation_from_rotation(video.rotation),
        );
    }
    if let Some(duration) = probe.format.duration {
        set_tag(media, "Duration", Value::from(duration));
    }
}

fn set_tag(media: &mut Value, key: &str, value: Value) {
    if let Some(map) = media.as_object_mut() {
        map.insert(key.into(), value);
    }
}

fn remove_tag(media: &mut Value, key: &str) {
    if let Some(map) = media.as_object_mut() {
        map.remove(key);
    }
}

fn orientation_from_rotation(rotation: i32) -> Value {
    Value::from(match rotation {
        -90 => 6,
        0 => 1,
        90 => 8,
        180 => 3,
        _ => 1,
    })
}

/// TypeScript updates width and height independently. An edited side is kept once it is set.
fn metadata_dimension_updates(
    is_edited: bool,
    current_width: Option<i32>,
    current_height: Option<i32>,
    decoded_width: Option<i32>,
    decoded_height: Option<i32>,
) -> (Option<i32>, Option<i32>) {
    let width = if !is_edited || current_width.is_none() {
        decoded_width
    } else {
        None
    };
    let height = if !is_edited || current_height.is_none() {
        decoded_height
    } else {
        None
    };
    (width, height)
}

fn image_dimensions(tags: &Value) -> (Option<i32>, Option<i32>) {
    if let Some(size) = tag_string(tags, "ImageSize") {
        let mut dimensions = size.split('x').map(str::trim);
        if let (Some(Ok(width)), Some(Ok(height))) = (
            dimensions.next().map(str::parse::<i32>),
            dimensions.next().map(str::parse::<i32>),
        ) {
            if width > 0 && height > 0 {
                return (Some(width), Some(height));
            }
        }
    }
    (
        tag_validated_i32(tags, "ImageWidth").or_else(|| tag_validated_i32(tags, "ExifImageWidth")),
        tag_validated_i32(tags, "ImageHeight")
            .or_else(|| tag_validated_i32(tags, "ExifImageHeight")),
    )
}

/// TypeScript `hasGeo`: drop only a missing pair or the `(0, 0)` placeholder.
fn gps_coordinates(tags: &Value) -> (Option<f64>, Option<f64>) {
    let Some(lat) = tag_f64(tags, "GPSLatitude").filter(|value| value.is_finite()) else {
        return (None, None);
    };
    let Some(lon) = tag_f64(tags, "GPSLongitude").filter(|value| value.is_finite()) else {
        return (None, None);
    };
    if lat == 0.0 && lon == 0.0 {
        (None, None)
    } else {
        (Some(lat), Some(lon))
    }
}

fn camera_make(tags: &Value) -> Option<String> {
    tag_string(tags, "Make")
        .or_else(|| tag_nested_string(tags, "Device", "Manufacturer"))
        .or_else(|| tag_string(tags, "AndroidMake"))
        .or_else(|| tag_string(tags, "DeviceManufacturer"))
}

fn camera_model(tags: &Value) -> Option<String> {
    tag_string(tags, "Model")
        .or_else(|| tag_nested_string(tags, "Device", "ModelName"))
        .or_else(|| tag_string(tags, "AndroidModel"))
        .or_else(|| tag_string(tags, "DeviceModelName"))
}

fn lens_model(tags: &Value) -> Option<String> {
    let lens_model = [
        tag_string(tags, "LensID"),
        tag_string(tags, "LensType"),
        tag_string(tags, "LensSpec"),
        tag_string(tags, "LensModel"),
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.is_empty())
    .unwrap_or_default()
    .trim()
    .to_string();

    if lens_model.is_empty() || lens_model == "----" || lens_model.starts_with("Unknown") {
        None
    } else {
        Some(lens_model)
    }
}

fn bits_per_sample(tags: &Value) -> Option<i32> {
    let candidates = [
        parse_bit_depth_tag(tags, "BitsPerSample"),
        parse_bit_depth_tag(tags, "ComponentBitDepth"),
        parse_bit_depth_tag(tags, "ImagePixelDepth"),
        parse_bit_depth_tag(tags, "BitDepth"),
        parse_bit_depth_tag(tags, "ColorBitDepth"),
    ];

    let mut bits_per_sample = candidates.into_iter().flatten().next()?;
    if bits_per_sample >= 24 && bits_per_sample % 3 == 0 {
        bits_per_sample /= 3;
    }
    Some(bits_per_sample)
}

fn parse_bit_depth_tag(tags: &Value, name: &str) -> Option<i32> {
    match tag_value(tags, name)? {
        Value::String(text) => text.split_whitespace().next()?.parse().ok(),
        Value::Number(number) => number.as_i64().map(|value| value as i32),
        Value::Array(items) => items.iter().find_map(|item| {
            item.as_i64()
                .map(|value| value as i32)
                .or_else(|| item.as_str().and_then(|text| text.parse().ok()))
        }),
        _ => None,
    }
}

fn tag_nested_string(tags: &Value, object: &str, field: &str) -> Option<String> {
    tags.get(object)
        .and_then(|value| value.get(field))
        .and_then(|value| match value {
            Value::String(text) if !text.is_empty() => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
}

/// Official `getTagList`: a present list wins even when it is empty.
/// `TagsList`, then `HierarchicalSubject` (`|` becomes `/`), then `Keywords`.
fn collect_tags(tags: &Value) -> Vec<String> {
    if let Some(items) = present_tag_items(tags, "TagsList") {
        return items.iter().filter_map(json_tag_string).collect();
    }
    if let Some(items) = present_tag_items(tags, "HierarchicalSubject") {
        return items.iter().filter_map(hierarchical_subject).collect();
    }
    if let Some(items) = present_tag_items(tags, "Keywords") {
        return items.iter().filter_map(json_tag_string).collect();
    }
    Vec::new()
}

fn present_tag_items(tags: &Value, name: &str) -> Option<Vec<Value>> {
    let value = tag_value(tags, name)?;
    Some(match value {
        Value::Array(items) => items,
        other => vec![other],
    })
}

fn json_tag_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn hierarchical_subject(value: &Value) -> Option<String> {
    if value.is_number() || value.is_boolean() {
        return json_tag_string(value);
    }
    let text = value.as_str()?;
    Some(
        text.split('|')
            .map(|part| part.replace('/', "|"))
            .collect::<Vec<_>>()
            .join("/"),
    )
}

#[derive(Debug, Clone)]
struct ExifDate {
    /// The absolute capture instant.
    instant: DateTime<Utc>,
    /// The capture's wall-clock date/time, represented in UTC for the
    /// timezone-agnostic `asset.localDateTime` database column.
    local: DateTime<Utc>,
    time_zone: Option<String>,
}

fn resolve_time_zone(
    tags: &Value,
    raw_date: Option<&str>,
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> Option<String> {
    tag_string(tags, "TimeZone")
        .or_else(|| tag_string(tags, "OffsetTime"))
        .or_else(|| {
            raw_date.and_then(|raw| {
                if raw.ends_with('Z') || raw.ends_with("+00:00") {
                    Some("UTC+0".to_string())
                } else {
                    None
                }
            })
        })
        .or_else(|| {
            latitude
                .zip(longitude)
                .and_then(|(lat, lon)| crate::utils::geo_tz::find_timezone(lon, lat))
        })
}

#[cfg(test)]
fn extract_exif_date(tags: &Value, resolved_time_zone: Option<&str>) -> Option<ExifDate> {
    let raw = EXIF_DATE_TAGS
        .iter()
        .find_map(|tag| tag_string(tags, tag))?;
    parse_exif_date(&raw, resolved_time_zone)
}

fn parse_exif_date(raw: &str, resolved_time_zone: Option<&str>) -> Option<ExifDate> {
    if let Ok(datetime) = DateTime::parse_from_rfc3339(raw) {
        return Some(ExifDate {
            instant: datetime.with_timezone(&Utc),
            local: Utc.from_utc_datetime(&datetime.naive_local()),
            time_zone: Some(offset_label(datetime.offset())),
        });
    }

    for format in ["%Y:%m:%d %H:%M:%S%.f%:z", "%Y:%m:%d %H:%M:%S%.f%z"] {
        if let Ok(datetime) = DateTime::parse_from_str(raw, format) {
            return Some(ExifDate {
                instant: datetime.with_timezone(&Utc),
                local: Utc.from_utc_datetime(&datetime.naive_local()),
                time_zone: Some(offset_label(datetime.offset())),
            });
        }
    }

    let local = parse_exif_local_datetime(raw)?;
    if let Some(time_zone) = resolved_time_zone {
        if let Ok(tz) = time_zone.parse::<Tz>() {
            let zoned = tz.from_local_datetime(&local).single()?;
            return Some(ExifDate {
                instant: zoned.with_timezone(&Utc),
                local: Utc.from_utc_datetime(&local),
                time_zone: Some(time_zone.to_string()),
            });
        }

        if let Some(offset) = parse_offset(time_zone) {
            let zoned = offset.from_local_datetime(&local).single()?;
            return Some(ExifDate {
                instant: zoned.with_timezone(&Utc),
                local: Utc.from_utc_datetime(&local),
                time_zone: Some(offset_label(&offset)),
            });
        }
    }

    Some(ExifDate {
        instant: Utc.from_utc_datetime(&local),
        local: Utc.from_utc_datetime(&local),
        time_zone: resolved_time_zone.map(str::to_string),
    })
}

fn parse_exif_local_datetime(raw: &str) -> Option<NaiveDateTime> {
    [
        "%Y:%m:%d %H:%M:%S%.f",
        "%Y:%m:%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(raw, format).ok())
}

fn parse_offset(value: &str) -> Option<FixedOffset> {
    value.parse().ok().or_else(|| {
        value
            .strip_prefix("UTC")
            .and_then(|offset| offset.parse().ok())
    })
}

fn offset_label(offset: &FixedOffset) -> String {
    let offset = offset.to_string();
    if offset == "+00:00" {
        "UTC+0".to_string()
    } else {
        offset
    }
}

fn earliest_file_date<const N: usize>(dates: [Option<DateTime<Utc>>; N]) -> Option<DateTime<Utc>> {
    dates.into_iter().flatten().min()
}

/// Official `getDates` fallback: ignore a zero birthtime, then take the earlier of
/// filesystem mtime and birthtime, then the earlier of that and `fileCreatedAt`.
/// The stored `fileModifiedAt` is not part of this fallback.
fn fallback_asset_date(
    file_created_at: Option<DateTime<Utc>>,
    birthtime: Option<DateTime<Utc>>,
    mtime: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    let filesystem = match birthtime.filter(|date| date.timestamp_millis() > 0) {
        Some(birth) => match mtime {
            Some(modified) => Some(birth.min(modified)),
            None => Some(birth),
        },
        None => mtime,
    };
    earliest_file_date([file_created_at, filesystem])
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
    use serde_json::json;

    use super::{
        apply_heif_orientation, bits_per_sample, camera_make, camera_model, collect_tags,
        earliest_file_date, extract_exif_date, fallback_asset_date, gps_coordinates,
        image_dimensions, lens_model, merge_sidecar_tags, metadata_dimension_updates,
        parse_exif_date, resolve_time_zone,
    };
    use crate::service::media::exiftool::{tag_validated_f64, tag_validated_i32};

    #[test]
    fn fallback_date_ignores_zero_birthtime_and_uses_file_created_at() {
        let file_created = Utc.with_ymd_and_hms(2020, 1, 2, 3, 4, 5).unwrap();
        let mtime = Utc.with_ymd_and_hms(2021, 6, 7, 8, 9, 10).unwrap();
        let epoch = DateTime::from_timestamp_millis(0).unwrap();
        assert_eq!(
            fallback_asset_date(Some(file_created), Some(epoch), Some(mtime)),
            Some(file_created)
        );

        let birth = Utc.with_ymd_and_hms(2019, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(
            fallback_asset_date(Some(file_created), Some(birth), Some(mtime)),
            Some(birth)
        );
    }

    #[test]
    fn preserves_exif_offset_and_local_wall_clock_time() {
        let date = extract_exif_date(
            &json!({
                "DateTimeOriginal": "2024:03:04 12:30:15+02:00"
            }),
            None,
        )
        .expect("EXIF date should parse");

        assert_eq!(date.instant.hour(), 10);
        assert_eq!(date.instant.minute(), 30);
        assert_eq!(date.local.hour(), 12);
        assert_eq!(date.local.minute(), 30);
        assert_eq!(date.time_zone.as_deref(), Some("+02:00"));
    }

    #[test]
    fn applies_separate_offset_time_to_timezone_less_exif_date() {
        let tags = json!({
            "DateTimeOriginal": "2024:03:04 12:30:15",
            "OffsetTime": "-05:00"
        });
        let time_zone = resolve_time_zone(&tags, Some("2024:03:04 12:30:15"), None, None);
        let date = extract_exif_date(&tags, time_zone.as_deref()).expect("EXIF date should parse");

        assert_eq!(date.instant.hour(), 17);
        assert_eq!(date.local.hour(), 12);
        assert_eq!(date.time_zone.as_deref(), Some("-05:00"));
    }

    #[test]
    fn uses_subsecond_date_time_original_before_fallback_tags() {
        let tags = json!({
            "SubSecDateTimeOriginal": "2024:03:04 12:30:15.987+00:00",
            "CreateDate": "2020:01:01 00:00:00"
        });
        let time_zone = resolve_time_zone(&tags, Some("2024:03:04 12:30:15.987+00:00"), None, None);
        let date = extract_exif_date(&tags, time_zone.as_deref()).expect("EXIF date should parse");

        assert_eq!(date.local.year(), 2024);
        assert_eq!(date.local.timestamp_subsec_millis(), 987);
        assert_eq!(date.time_zone.as_deref(), Some("UTC+0"));
    }

    #[test]
    fn sidecar_date_replaces_media_date_without_importing_sidecar_duration() {
        let mut media = json!({
            "DateTimeOriginal": "2020:01:01 00:00:00",
            "OffsetTime": "+00:00",
            "Duration": 1.0
        });
        let sidecar = json!({
            "DateTimeOriginal": "2024:03:04 12:30:15+02:00",
            "Duration": 999.0,
            "Description": "from sidecar"
        });

        merge_sidecar_tags(&mut media, &sidecar);

        let date = extract_exif_date(&media, None).expect("sidecar date should remain");
        assert_eq!(date.local.year(), 2024);
        assert!(media.get("Duration").is_some());
        assert_eq!(media["Description"], "from sidecar");
    }

    #[test]
    fn uses_image_size_and_heif_rotation_for_metadata() {
        let mut tags = json!({
            "ImageSize": "6000x4000",
            "ImageWidth": 100,
            "ImageHeight": 100,
            "Rotation": 3,
            "Orientation": 1
        });

        apply_heif_orientation(&mut tags, "photo.heic");
        assert_eq!(image_dimensions(&tags), (Some(6000), Some(4000)));
        assert_eq!(tags["Orientation"], 6);
    }

    #[test]
    fn numeric_exif_lists_use_the_first_value() {
        let tags = json!({
            "ImageWidth": [6000, 160],
            "ImageHeight": [4000, 120],
            "ISO": [200, 400],
            "FNumber": [2.8, 4.0],
            "Orientation": [6, 1]
        });
        assert_eq!(image_dimensions(&tags), (Some(6000), Some(4000)));
        assert_eq!(tag_validated_i32(&tags, "ISO"), Some(200));
        assert_eq!(tag_validated_f64(&tags, "FNumber"), Some(2.8));
        assert_eq!(tag_validated_i32(&tags, "Orientation"), Some(6));
    }

    #[test]
    fn falls_back_to_the_earliest_available_file_timestamp() {
        let upload_time = Utc.with_ymd_and_hms(2024, 3, 5, 12, 0, 0).unwrap();
        let birth_time = Utc.with_ymd_and_hms(2021, 1, 1, 12, 0, 0).unwrap();
        let modify_time = Utc.with_ymd_and_hms(2020, 1, 1, 12, 0, 0).unwrap();

        assert_eq!(
            earliest_file_date([Some(upload_time), Some(birth_time), Some(modify_time), None]),
            Some(modify_time)
        );
    }

    #[test]
    fn reads_camera_make_model_and_lens_metadata() {
        let tags = json!({
            "Device": { "Manufacturer": "icc-make", "ModelName": "icc-model" },
            "LensID": "24-70mm",
            "BitsPerSample": 24
        });

        assert_eq!(camera_make(&tags).as_deref(), Some("icc-make"));
        assert_eq!(camera_model(&tags).as_deref(), Some("icc-model"));
        assert_eq!(lens_model(&tags).as_deref(), Some("24-70mm"));
        assert_eq!(bits_per_sample(&tags), Some(8));
    }

    #[test]
    fn ignores_unknown_lens_identifiers() {
        assert!(lens_model(&json!({ "LensID": "----" })).is_none());
        assert!(lens_model(&json!({ "LensID": "Unknown (0 ff ff)" })).is_none());
    }

    #[test]
    fn infers_timezone_from_gps_coordinates_for_naive_exif_date() {
        let tags = json!({
            "GPSDateTime": "2023:11:15 04:30:00",
            "GPSLatitude": 34.0522,
            "GPSLongitude": -118.2437
        });
        let time_zone = resolve_time_zone(
            &tags,
            Some("2023:11:15 04:30:00"),
            Some(34.0522),
            Some(-118.2437),
        );
        let date = parse_exif_date("2023:11:15 04:30:00", time_zone.as_deref())
            .expect("GPS date should parse");

        assert_eq!(time_zone.as_deref(), Some("America/Los_Angeles"));
        assert_eq!(date.instant.hour(), 12);
        assert_eq!(date.instant.minute(), 30);
        assert_eq!(date.local.hour(), 4);
        assert_eq!(date.local.minute(), 30);
        assert_eq!(date.time_zone.as_deref(), Some("America/Los_Angeles"));
    }

    #[test]
    fn treats_explicit_utc_zero_suffix_as_utc_plus_zero() {
        let time_zone = resolve_time_zone(
            &json!({}),
            Some("2024-09-01T00:00:00.000+00:00"),
            None,
            None,
        );
        assert_eq!(time_zone.as_deref(), Some("UTC+0"));
    }

    #[test]
    fn keeps_equator_or_prime_meridian_gps_and_parses_channel_bit_depth() {
        assert_eq!(
            gps_coordinates(&json!({"GPSLatitude": 0.0, "GPSLongitude": 10.5})),
            (Some(0.0), Some(10.5))
        );
        assert_eq!(
            gps_coordinates(&json!({"GPSLatitude": 51.5, "GPSLongitude": 0.0})),
            (Some(51.5), Some(0.0))
        );
        assert_eq!(
            gps_coordinates(&json!({"GPSLatitude": 0, "GPSLongitude": 0})),
            (None, None)
        );

        assert_eq!(
            bits_per_sample(&json!({"BitsPerSample": "16 16 16"})),
            Some(16)
        );
        assert_eq!(
            bits_per_sample(&json!({"BitsPerSample": "24 24 24"})),
            Some(8)
        );
        assert_eq!(
            bits_per_sample(&json!({"BitsPerSample": [8, 8, 8]})),
            Some(8)
        );
    }

    #[test]
    fn edited_dimensions_update_each_side_independently() {
        assert_eq!(
            metadata_dimension_updates(false, Some(100), Some(200), Some(300), Some(400)),
            (Some(300), Some(400))
        );
        assert_eq!(
            metadata_dimension_updates(true, Some(100), Some(200), Some(300), Some(400)),
            (None, None)
        );
        assert_eq!(
            metadata_dimension_updates(true, Some(100), None, Some(300), Some(400)),
            (None, Some(400))
        );
        assert_eq!(
            metadata_dimension_updates(true, None, Some(200), Some(300), Some(400)),
            (Some(300), None)
        );
    }

    #[test]
    fn empty_tag_list_does_not_fall_through_and_numbers_are_kept() {
        assert!(
            collect_tags(&json!({
                "TagsList": [],
                "HierarchicalSubject": ["Place|City"]
            }))
            .is_empty()
        );
        assert_eq!(
            collect_tags(&json!({ "TagsList": [1, "beach"] })),
            vec!["1".to_string(), "beach".to_string()]
        );
        assert_eq!(
            collect_tags(&json!({ "HierarchicalSubject": ["Place|City/Old", 7] })),
            vec!["Place/City|Old".to_string(), "7".to_string()]
        );
        assert_eq!(
            collect_tags(&json!({ "Keywords": "sunset" })),
            vec!["sunset".to_string()]
        );
    }
}

fn system_time_to_utc(time: std::time::SystemTime) -> Option<DateTime<Utc>> {
    let duration = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    Utc.timestamp_opt(duration.as_secs() as i64, duration.subsec_nanos())
        .single()
}
