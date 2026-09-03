use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, FixedOffset, Utc};
use chrono_tz::Tz;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::assets;
use crate::models::db::sidecar_job::{self, SidecarWriteAsset};
use crate::service::job::JobService;
use crate::service::media::exiftool::{self, TagWriteValue};

const JOBS_BATCH_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarCheckOutcome {
    NotFound,
    Skipped,
    Success,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarWriteOutcome {
    Failed,
    Skipped,
    Success,
}

#[derive(Clone)]
pub struct SidecarService {
    pool: PgPool,
    jobs: JobService,
}

impl SidecarService {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self { pool, jobs }
    }

    pub async fn queue_all(&self, force: bool) -> Result<(), String> {
        let asset_ids = sidecar_job::stream_for_sidecar(&self.pool, force)
            .await
            .map_err(|err| err.to_string())?;

        for chunk in asset_ids.chunks(JOBS_BATCH_SIZE) {
            for asset_id in chunk {
                self.jobs
                    .queue_sidecar_check(asset_id)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        Ok(())
    }

    pub async fn check_sidecar(
        &self,
        asset_id: &Uuid,
        source: Option<&str>,
    ) -> Result<SidecarCheckOutcome, String> {
        let Some(asset) = sidecar_job::get_for_sidecar_check(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(SidecarCheckOutcome::NotFound);
        };

        let mut sidecar_path = None;
        for candidate in sidecar_candidates(&asset.original_path, asset.sidecar_path.as_deref()) {
            if tokio::fs::metadata(&candidate).await.is_ok() {
                sidecar_path = Some(candidate);
                break;
            }
        }

        let is_changed = sidecar_path.as_deref() != asset.sidecar_path.as_deref();
        if !is_changed {
            self.queue_metadata_after_check(asset_id, source).await?;
            return Ok(SidecarCheckOutcome::Skipped);
        }

        if sidecar_path.is_none() {
            sidecar_job::delete_sidecar_file(&self.pool, asset_id)
                .await
                .map_err(|err| err.to_string())?;
        } else if let Some(path) = sidecar_path.as_ref() {
            assets::upsert_sidecar_file(&self.pool, asset_id, path)
                .await
                .map_err(|err| err.to_string())?;
        }

        self.queue_metadata_after_check(asset_id, source).await?;
        Ok(SidecarCheckOutcome::Success)
    }

    pub async fn write_sidecar(&self, asset_id: &Uuid) -> Result<SidecarWriteOutcome, String> {
        let Some(asset) = sidecar_job::get_for_sidecar_write(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(SidecarWriteOutcome::Failed);
        };

        let locked_properties = sidecar_job::get_locked_properties(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?;

        let sidecar_path = asset
            .sidecar_path
            .clone()
            .unwrap_or_else(|| format!("{}.xmp", asset.original_path));

        let tags = build_write_tags(&asset, &locked_properties);
        if tags.is_empty() {
            self.queue_metadata_after_write(asset_id).await?;
            return Ok(SidecarWriteOutcome::Skipped);
        }

        exiftool::write_tags(&sidecar_path, &tags).await?;

        if asset.sidecar_path.is_none() {
            assets::upsert_sidecar_file(&self.pool, asset_id, &sidecar_path)
                .await
                .map_err(|err| err.to_string())?;
        }

        sidecar_job::unlock_properties(&self.pool, asset_id, &locked_properties)
            .await
            .map_err(|err| err.to_string())?;

        self.queue_metadata_after_write(asset_id).await?;
        Ok(SidecarWriteOutcome::Success)
    }

    async fn queue_metadata_after_check(
        &self,
        asset_id: &Uuid,
        source: Option<&str>,
    ) -> Result<(), String> {
        match source {
            Some(source) => self
                .jobs
                .queue_asset_extract_metadata_with_source(asset_id, source)
                .await
                .map_err(|err| err.to_string()),
            None => self
                .jobs
                .queue_asset_extract_metadata(asset_id)
                .await
                .map_err(|err| err.to_string()),
        }
    }

    async fn queue_metadata_after_write(&self, asset_id: &Uuid) -> Result<(), String> {
        self.jobs
            .queue_asset_extract_metadata_with_source(asset_id, "sidecar-write")
            .await
            .map_err(|err| err.to_string())
    }
}

fn sidecar_candidates(original_path: &str, existing_sidecar: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(path) = existing_sidecar {
        candidates.push(path.to_string());
    }
    candidates.push(format!("{original_path}.xmp"));

    let path = Path::new(original_path);
    if let (Some(parent), Some(stem)) = (
        path.parent(),
        path.file_stem().and_then(|value| value.to_str()),
    ) {
        candidates.push(
            parent
                .join(format!("{stem}.xmp"))
                .to_string_lossy()
                .into_owned(),
        );
    }

    candidates
}

fn build_write_tags(
    asset: &SidecarWriteAsset,
    locked_properties: &[String],
) -> Vec<(&'static str, TagWriteValue)> {
    let mut values: HashMap<&'static str, TagWriteValue> = HashMap::new();
    let locked: std::collections::HashSet<&str> =
        locked_properties.iter().map(String::as_str).collect();

    if locked.contains("description") {
        values.insert(
            "Description",
            TagWriteValue::Text(asset.description.clone()),
        );
        values.insert(
            "ImageDescription",
            TagWriteValue::Text(asset.description.clone()),
        );
    }

    if locked.contains("dateTimeOriginal") || locked.contains("timeZone") {
        if let Some(formatted) =
            merge_time_zone(asset.date_time_original, asset.time_zone.as_deref())
        {
            values.insert("DateTimeOriginal", TagWriteValue::Text(formatted));
        }
    }

    if locked.contains("latitude") {
        if let Some(latitude) = asset.latitude {
            values.insert("GPSLatitude", TagWriteValue::Number(latitude));
        }
    }

    if locked.contains("longitude") {
        if let Some(longitude) = asset.longitude {
            values.insert("GPSLongitude", TagWriteValue::Number(longitude));
        }
    }

    if locked.contains("rating") {
        values.insert(
            "Rating",
            TagWriteValue::Number(f64::from(asset.rating.unwrap_or(0))),
        );
    }

    if locked.contains("tags") {
        if let Some(tags) = asset.tags.clone() {
            values.insert("TagsList", TagWriteValue::StringList(tags));
        }
    }

    values
        .into_iter()
        .filter(|(_, value)| !tag_value_is_empty(value))
        .collect()
}

fn tag_value_is_empty(value: &TagWriteValue) -> bool {
    match value {
        TagWriteValue::Text(_) | TagWriteValue::Number(_) => false,
        TagWriteValue::StringList(items) => items.is_empty(),
    }
}

fn merge_time_zone(
    date_time_original: Option<DateTime<Utc>>,
    time_zone: Option<&str>,
) -> Option<String> {
    let date_time = date_time_original?;
    let Some(time_zone) = time_zone else {
        return Some(date_time.to_rfc3339());
    };

    if let Ok(tz) = time_zone.parse::<Tz>() {
        return Some(date_time.with_timezone(&tz).to_rfc3339());
    }

    let offset = time_zone.parse::<FixedOffset>().ok().or_else(|| {
        time_zone
            .strip_prefix("UTC")
            .and_then(|value| value.parse().ok())
    })?;
    Some(date_time.with_timezone(&offset).to_rfc3339())
}
