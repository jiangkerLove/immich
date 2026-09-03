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
use crate::utils::fs_access::has_read_access;

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

        let sidecar_path =
            first_readable_sidecar(&asset.original_path, asset.sidecar_path.as_deref());

        let is_changed = is_sidecar_changed(sidecar_path.as_deref(), asset.sidecar_path.as_deref());
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

fn is_sidecar_changed(disk: Option<&str>, db: Option<&str>) -> bool {
    match (disk, db) {
        (Some(disk), Some(db)) => disk != db,
        _ => true,
    }
}

fn first_readable_sidecar(original_path: &str, existing_sidecar: Option<&str>) -> Option<String> {
    sidecar_candidates(original_path, existing_sidecar)
        .into_iter()
        .find(|candidate| has_read_access(candidate))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_changed_matches_typescript_null_undefined_semantics() {
        assert!(is_sidecar_changed(None, None));
        assert!(is_sidecar_changed(Some("/a.xmp"), None));
        assert!(is_sidecar_changed(None, Some("/a.xmp")));
        assert!(!is_sidecar_changed(Some("/a.xmp"), Some("/a.xmp")));
        assert!(is_sidecar_changed(Some("/a.xmp"), Some("/b.xmp")));
    }

    #[test]
    fn sidecar_candidates_match_typescript_order() {
        let candidates = sidecar_candidates("/photos/IMG_123.jpg", Some("/custom/photo.xmp"));
        assert_eq!(
            candidates,
            vec![
                "/custom/photo.xmp".to_string(),
                "/photos/IMG_123.jpg.xmp".to_string(),
                "/photos/IMG_123.xmp".to_string(),
            ]
        );
    }

    #[test]
    fn first_readable_sidecar_requires_read_access() {
        let dir = tempfile::tempdir().expect("temp dir");
        let original = dir.path().join("IMG_123.jpg");
        let alongside = dir.path().join("IMG_123.jpg.xmp");
        std::fs::write(&original, b"jpg").expect("write original");
        std::fs::write(&alongside, b"xmp").expect("write sidecar");

        let found = first_readable_sidecar(original.to_str().unwrap(), None);
        assert_eq!(found.as_deref(), alongside.to_str());

        let missing = first_readable_sidecar(dir.path().join("none.jpg").to_str().unwrap(), None);
        assert_eq!(missing, None);
    }

    #[cfg(unix)]
    #[test]
    fn skips_unreadable_sidecar_candidates_when_not_root() {
        if unsafe { libc::geteuid() == 0 } {
            return;
        }

        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let original = dir.path().join("IMG_123.jpg");
        let existing = dir.path().join("secret.xmp");
        let alongside = dir.path().join("IMG_123.jpg.xmp");
        std::fs::write(&original, b"jpg").expect("write original");
        std::fs::write(&existing, b"old").expect("write existing sidecar");
        std::fs::write(&alongside, b"new").expect("write alongside sidecar");

        let mut permissions = std::fs::metadata(&existing)
            .expect("metadata")
            .permissions();
        permissions.set_mode(0o000);
        std::fs::set_permissions(&existing, permissions).expect("chmod");

        let found = first_readable_sidecar(original.to_str().unwrap(), existing.to_str());
        assert_eq!(found.as_deref(), alongside.to_str());
    }
}
