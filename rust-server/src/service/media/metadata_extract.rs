use std::path::Path;

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::map;
use crate::models::db::metadata_job::{
    self, UpdateAssetAfterMetadata, UpsertAssetAudio, UpsertAssetExif, UpsertAssetVideo,
};
use crate::models::db::system_metadata::get_json;
use crate::service::job::EntityJob;
use crate::service::job::JobService;
use crate::service::media::exiftool::{self, tag_f64, tag_i32, tag_string, tag_string_list};
use crate::service::media::ffprobe::{self, ProbeResult};

const JOBS_BATCH_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataExtractOutcome {
    Success,
    Failed,
}

#[derive(Clone)]
pub struct MetadataExtractService {
    pool: PgPool,
    jobs: JobService,
}

impl MetadataExtractService {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self { pool, jobs }
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
            return Ok(MetadataExtractOutcome::Failed);
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

        if let Some(probe) = probe.as_ref() {
            merge_probe_tags(&mut media_tags, probe);
        }

        let metadata = tokio::fs::metadata(&asset.original_path)
            .await
            .map_err(|err| err.to_string())?;
        let modify_date = metadata.modified().ok().and_then(system_time_to_utc);
        let file_size = metadata.len() as i64;

        let (width, height) = image_dimensions(&media_tags);
        let orientation = tag_i32(&media_tags, "Orientation").map(|v| v.to_string());
        let is_sideways = orientation
            .as_deref()
            .is_some_and(|v| matches!(v, "5" | "6" | "7" | "8" | "90" | "-90"));
        let asset_width = if is_sideways { height } else { width };
        let asset_height = if is_sideways { width } else { height };

        let (latitude, longitude) = gps_coordinates(&media_tags);
        let (city, state, country) = self.reverse_geocode(latitude, longitude).await?;

        let tags = collect_tags(&media_tags);
        let exif = UpsertAssetExif {
            asset_id: asset.id,
            make: tag_string(&media_tags, "Make").or_else(|| tag_string(&media_tags, "AndroidMake")),
            model: tag_string(&media_tags, "Model").or_else(|| tag_string(&media_tags, "AndroidModel")),
            exif_image_width: width,
            exif_image_height: height,
            file_size_in_byte: Some(file_size),
            orientation,
            date_time_original: parse_exif_datetime(&media_tags, "DateTimeOriginal")
                .or_else(|| parse_exif_datetime(&media_tags, "CreateDate")),
            modify_date,
            lens_model: tag_string(&media_tags, "LensModel"),
            f_number: tag_f64(&media_tags, "FNumber"),
            focal_length: tag_f64(&media_tags, "FocalLength"),
            iso: tag_i32(&media_tags, "ISO"),
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
            time_zone: tag_string(&media_tags, "OffsetTime")
                .or_else(|| tag_string(&media_tags, "TimeZone")),
            projection_type: tag_string(&media_tags, "ProjectionType").map(|v| v.to_ascii_uppercase()),
            profile_description: tag_string(&media_tags, "ProfileDescription"),
            colorspace: tag_string(&media_tags, "ColorSpace"),
            bits_per_sample: tag_i32(&media_tags, "BitsPerSample"),
            auto_stack_id: tag_string(&media_tags, "BurstID")
                .or_else(|| tag_string(&media_tags, "BurstUUID"))
                .or_else(|| tag_string(&media_tags, "MediaUniqueID")),
            rating: tag_i32(&media_tags, "Rating").filter(|v| (1..=5).contains(v)),
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
                codec_name: video.codec_name.clone(),
                format_name: probe.format.format_name.clone(),
                format_long_name: probe.format.format_long_name.clone(),
                pixel_format: video.pixel_format.clone(),
            })
        });

        let audio = probe.as_ref().and_then(|p| p.audio.as_ref()).map(|audio| {
            UpsertAssetAudio {
                asset_id: asset.id,
                bitrate: audio.bitrate.clamp(0, i64::from(i32::MAX)) as i32,
                index: audio.index as i16,
                profile: audio.profile.map(|v| v as i16),
                codec_name: audio.codec_name.clone(),
            }
        });

        let duration_ms = probe
            .as_ref()
            .and_then(|p| p.format.duration)
            .map(|seconds| (seconds * 1000.0).round() as i64)
            .or_else(|| tag_f64(&media_tags, "Duration").map(|s| (s * 1000.0).round() as i64));

        let local_date_time = exif
            .date_time_original
            .or(asset.file_created_at)
            .or(asset.file_modified_at);

        let update_dims = (!asset.is_edited || asset.width.is_none() || asset.height.is_none())
            .then_some((asset_width, asset_height));

        metadata_job::upsert_metadata(
            &self.pool,
            &exif,
            video.as_ref(),
            audio.as_ref(),
            &UpdateAssetAfterMetadata {
                asset_id: asset.id,
                duration: duration_ms,
                local_date_time,
                file_created_at: exif.date_time_original.or(asset.file_created_at),
                file_modified_at: modify_date.or(asset.file_modified_at),
                width: update_dims.and_then(|(w, _)| w),
                height: update_dims.and_then(|(_, h)| h),
            },
        )
        .await
        .map_err(|err| err.to_string())?;

        self.queue_follow_up_jobs(job).await?;
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

        let config = get_json(&self.pool, "system-config")
            .await
            .map_err(|err| err.to_string())?;
        let enabled = config
            .as_ref()
            .and_then(|value| value.get("reverseGeocoding"))
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
        if job.source.as_deref() == Some("sidecar-write") {
            return Ok(());
        }

        let config = get_json(&self.pool, "system-config")
            .await
            .map_err(|err| err.to_string())?;
        let template_enabled = config
            .as_ref()
            .and_then(|value| value.get("storageTemplate"))
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
                .queue_asset_generate_thumbnails_with_notify(
                    &job.id,
                    job.notify.unwrap_or(false),
                )
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(())
    }
}

fn merge_sidecar_tags(media: &mut Value, sidecar: &Value) {
    if let Some(obj) = sidecar.as_object() {
        for (key, value) in obj {
            if key == "SourceFile" || key == "ExifToolVersion" {
                continue;
            }
            if let Some(map) = media.as_object_mut() {
                map.insert(key.clone(), value.clone());
            }
        }
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
        set_tag(media, "Orientation", orientation_from_rotation(video.rotation));
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

fn orientation_from_rotation(rotation: i32) -> Value {
    Value::from(match rotation {
        -90 => 6,
        0 => 1,
        90 => 8,
        180 => 3,
        _ => 1,
    })
}

fn image_dimensions(tags: &Value) -> (Option<i32>, Option<i32>) {
    (
        tag_i32(tags, "ImageWidth").or_else(|| tag_i32(tags, "ExifImageWidth")),
        tag_i32(tags, "ImageHeight").or_else(|| tag_i32(tags, "ExifImageHeight")),
    )
}

fn gps_coordinates(tags: &Value) -> (Option<f64>, Option<f64>) {
    let lat = tag_f64(tags, "GPSLatitude");
    let lon = tag_f64(tags, "GPSLongitude");
    if lat.is_some() && lon.is_some() && lat != Some(0.0) && lon != Some(0.0) {
        (lat, lon)
    } else {
        (None, None)
    }
}

fn collect_tags(tags: &Value) -> Vec<String> {
    let mut result = tag_string_list(tags, "TagsList");
    if result.is_empty() {
        result = tag_string_list(tags, "Keywords");
    }
    if result.is_empty() {
        result = tag_string_list(tags, "HierarchicalSubject");
    }
    result
}

fn parse_exif_datetime(tags: &Value, name: &str) -> Option<DateTime<Utc>> {
    let raw = tag_string(tags, name)?;
    for format in [
        "%Y:%m:%d %H:%M:%S%z",
        "%Y:%m:%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.fZ",
        "%Y-%m-%dT%H:%M:%S%.f%z",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(&raw, format) {
            return Some(Utc.from_utc_datetime(&dt));
        }
    }
    DateTime::parse_from_rfc3339(&raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn system_time_to_utc(time: std::time::SystemTime) -> Option<DateTime<Utc>> {
    let duration = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    Utc.timestamp_opt(duration.as_secs() as i64, duration.subsec_nanos())
        .single()
}
