use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Timelike, Utc};
use handlebars::Handlebars;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::assets;
use crate::models::db::move_history;
use crate::models::db::storage_template_job::{self, StorageTemplateAsset};
use crate::models::db::system_metadata::get_json;
use crate::models::db::users::UserDb;
use crate::service::job::{EntityJob, JobService};
use crate::utils::storage::StoragePaths;
use crate::utils::storage_move::{move_file, MoveFileOptions, MoveFileOutcome};

const LUXON_TOKENS: &[&str] = &[
    "s", "ss", "SSS", "m", "mm", "d", "dd", "W", "WW", "h", "hh", "H", "HH", "y", "yy", "M", "MM", "MMM", "MMMM",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageTemplateOutcome {
    Success,
    Skipped,
    Failed,
}

#[derive(Clone)]
pub struct StorageTemplateService {
    pool: PgPool,
    storage: StoragePaths,
    jobs: JobService,
}

impl StorageTemplateService {
    pub fn new(pool: PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self {
            pool,
            storage,
            jobs,
        }
    }

    pub async fn migrate_single(
        &self,
        asset_id: &Uuid,
        job: &EntityJob,
    ) -> Result<StorageTemplateOutcome, String> {
        let config = get_json(&self.pool, "system-config")
            .await
            .map_err(|err| err.to_string())?;
        let template_cfg = config
            .as_ref()
            .and_then(|value| value.get("storageTemplate"));
        let enabled = template_cfg
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !enabled {
            return Ok(StorageTemplateOutcome::Skipped);
        }

        let template = template_cfg
            .and_then(|value| value.get("template"))
            .and_then(|value| value.as_str())
            .unwrap_or("{{y}}/{{y}}-{{MM}}-{{dd}}/{{filename}}");
        let hash_verification = template_cfg
            .and_then(|value| value.get("hashVerificationEnabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);

        let Some(asset) = storage_template_job::get_for_storage_template_job(&self.pool, asset_id, false)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(StorageTemplateOutcome::Failed);
        };

        let user = UserDb::select_full_by_id(&self.pool, &asset.owner_id)
            .await
            .map_err(|err| err.to_string())?;
        let storage_label = user.and_then(|user| user.storage_label);

        let filename = if asset.original_file_name.is_empty() {
            asset.id.to_string()
        } else {
            asset.original_file_name.clone()
        };

        self.move_asset(
            &asset,
            storage_label.as_deref(),
            &filename,
            None,
            template,
            hash_verification,
        )
        .await?;

        if let Some(motion_id) = asset.live_photo_video_id {
            let Some(motion_asset) =
                storage_template_job::get_for_storage_template_job(&self.pool, &motion_id, true)
                    .await
                    .map_err(|err| err.to_string())?
            else {
                return Ok(StorageTemplateOutcome::Failed);
            };
            let motion_filename = live_photo_motion_filename(&filename, &motion_asset.original_path);
            self.move_asset(
                &motion_asset,
                storage_label.as_deref(),
                &motion_filename,
                Some(&asset),
                template,
                hash_verification,
            )
            .await?;
        }

        if matches!(job.source.as_deref(), Some("upload") | Some("copy")) {
            self.jobs
                .queue_asset_generate_thumbnails_with_notify(&job.id, job.notify.unwrap_or(false))
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(StorageTemplateOutcome::Success)
    }

    pub async fn migrate_all(&self) -> Result<StorageTemplateOutcome, String> {
        let config = get_json(&self.pool, "system-config")
            .await
            .map_err(|err| err.to_string())?;
        let template_cfg = config
            .as_ref()
            .and_then(|value| value.get("storageTemplate"));
        let enabled = template_cfg
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !enabled {
            return Ok(StorageTemplateOutcome::Skipped);
        }

        let template = template_cfg
            .and_then(|value| value.get("template"))
            .and_then(|value| value.as_str())
            .unwrap_or("{{y}}/{{y}}-{{MM}}-{{dd}}/{{filename}}");
        let hash_verification = template_cfg
            .and_then(|value| value.get("hashVerificationEnabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);

        move_history::clean_move_history(&self.pool)
            .await
            .map_err(|err| err.to_string())?;

        let assets = storage_template_job::stream_for_storage_template_job(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        let mut storage_labels: HashMap<Uuid, Option<String>> = HashMap::new();

        for asset in assets {
            let storage_label = self
                .storage_label_for_owner(&mut storage_labels, &asset.owner_id)
                .await?;

            let filename = if asset.original_file_name.is_empty() {
                asset.id.to_string()
            } else {
                asset.original_file_name.clone()
            };

            self.move_asset(
                &asset,
                storage_label.as_deref(),
                &filename,
                None,
                template,
                hash_verification,
            )
            .await?;

            if let Some(motion_id) = asset.live_photo_video_id {
                let Some(motion_asset) =
                    storage_template_job::get_for_storage_template_job(&self.pool, &motion_id, true)
                        .await
                        .map_err(|err| err.to_string())?
                else {
                    continue;
                };
                let motion_filename =
                    live_photo_motion_filename(&filename, &motion_asset.original_path);
                self.move_asset(
                    &motion_asset,
                    storage_label.as_deref(),
                    &motion_filename,
                    Some(&asset),
                    template,
                    hash_verification,
                )
                .await?;
            }
        }

        StoragePaths::remove_empty_dirs(&self.storage.library_base(), false)
            .await?;

        Ok(StorageTemplateOutcome::Success)
    }

    async fn storage_label_for_owner(
        &self,
        cache: &mut HashMap<Uuid, Option<String>>,
        owner_id: &Uuid,
    ) -> Result<Option<String>, String> {
        if let Some(label) = cache.get(owner_id) {
            return Ok(label.clone());
        }

        let label = UserDb::select_full_by_id(&self.pool, owner_id)
            .await
            .map_err(|err| err.to_string())?
            .and_then(|user| user.storage_label);
        cache.insert(*owner_id, label.clone());
        Ok(label)
    }

    async fn move_asset(
        &self,
        asset: &StorageTemplateAsset,
        storage_label: Option<&str>,
        filename: &str,
        still_photo: Option<&StorageTemplateAsset>,
        template: &str,
        hash_verification: bool,
    ) -> Result<(), String> {
        if asset.is_external || is_android_motion_path(&self.storage, &asset.original_path) {
            return Ok(());
        }

        let Some(file_size) = asset.file_size_in_byte else {
            eprintln!(
                "storage template: asset {} missing file size, skipping migration",
                asset.id
            );
            return Ok(());
        };

        let old_path = asset.original_path.clone();
        let new_path = self
            .get_template_path(asset, storage_label, filename, still_photo, template)
            .await?;

        if old_path == new_path {
            return Ok(());
        }

        self.move_original_file(asset, &old_path, &new_path, file_size, hash_verification)
            .await?;

        if let Some(sidecar_path) = assets::get_asset_file_path(&self.pool, &asset.id, "sidecar")
            .await
            .map_err(|err| err.to_string())?
        {
            let sidecar_new = format!("{new_path}.xmp");
            if sidecar_path != sidecar_new {
                let outcome = move_file(
                    &self.pool,
                    MoveFileOptions {
                        entity_id: asset.id,
                        path_type: "sidecar".to_string(),
                        old_path: Some(sidecar_path.clone()),
                        new_path: sidecar_new.clone(),
                        expected_size: None,
                        expected_checksum: None,
                        hash_verification: false,
                    },
                )
                .await?;

                if outcome == MoveFileOutcome::Completed {
                    assets::upsert_sidecar_file(&self.pool, &asset.id, &sidecar_new)
                        .await
                        .map_err(|err| err.to_string())?;
                }
            }
        }

        Ok(())
    }

    async fn move_original_file(
        &self,
        asset: &StorageTemplateAsset,
        old_path: &str,
        new_path: &str,
        file_size: i64,
        hash_verification: bool,
    ) -> Result<(), String> {
        if !path_exists(old_path).await {
            if path_exists(new_path).await {
                storage_template_job::update_original_path(&self.pool, &asset.id, new_path)
                    .await
                    .map_err(|err| err.to_string())?;
            }
            return Ok(());
        }

        let outcome = move_file(
            &self.pool,
            MoveFileOptions {
                entity_id: asset.id,
                path_type: "original".to_string(),
                old_path: Some(old_path.to_string()),
                new_path: new_path.to_string(),
                expected_size: Some(file_size),
                expected_checksum: Some(asset.checksum.clone()),
                hash_verification,
            },
        )
        .await?;

        if outcome == MoveFileOutcome::Completed {
            storage_template_job::update_original_path(&self.pool, &asset.id, new_path)
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(())
    }

    async fn get_template_path(
        &self,
        asset: &StorageTemplateAsset,
        storage_label: Option<&str>,
        filename: &str,
        still_photo: Option<&StorageTemplateAsset>,
        template: &str,
    ) -> Result<String, String> {
        let source = Path::new(&asset.original_path);
        let mut extension = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        extension = normalize_extension(&extension);

        let filename_without_extension = Path::new(filename)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(filename);
        let sanitized = sanitize_filename(filename_without_extension);

        let root_path = self
            .storage
            .library_folder(&asset.owner_id, storage_label);
        let root_path_str = normalize_path_string(&root_path);

        let asset_for_metadata = still_photo.unwrap_or(asset);
        let album_name = if template.contains("album") {
            get_album_name(&self.pool, &asset_for_metadata.owner_id, &asset_for_metadata.id).await?
        } else {
            None
        };

        let rendered = render_template(
            template,
            asset_for_metadata,
            &sanitized,
            &extension,
            album_name.as_deref(),
            None,
            None,
        )?;

        let full_path = join_library_path(&root_path, &rendered);
        let full_path_str = normalize_path_string(&full_path);

        if !full_path_str.starts_with(&root_path_str) {
            eprintln!(
                "storage template: invalid path {full_path_str}, expected prefix {root_path_str}"
            );
            return Ok(asset.original_path.clone());
        }

        let mut destination = format!("{full_path_str}.{extension}");
        let source_normalized = normalize_path_string(Path::new(&asset.original_path));

        if source_normalized == destination {
            return Ok(source_normalized);
        }

        if source_normalized.starts_with(&full_path_str)
            && source_normalized.ends_with(&format!(".{extension}"))
        {
            let diff = source_normalized
                .strip_prefix(&full_path_str)
                .unwrap_or("")
                .strip_suffix(&format!(".{extension}"))
                .unwrap_or("");
            if diff.starts_with('+') && diff[1..].chars().all(|c| c.is_ascii_digit()) {
                return Ok(source_normalized);
            }
        }

        let mut duplicate_count = 0;
        while path_exists(&destination).await {
            duplicate_count += 1;
            destination = format!("{full_path_str}+{duplicate_count}.{extension}");
        }

        Ok(destination)
    }
}

fn render_template(
    template: &str,
    asset: &StorageTemplateAsset,
    filename: &str,
    extension: &str,
    album_name: Option<&str>,
    album_start_date: Option<DateTime<Utc>>,
    album_end_date: Option<DateTime<Utc>>,
) -> Result<String, String> {
    let file_created_at = asset.file_created_at.unwrap_or_else(Utc::now);
    let mut substitutions = std::collections::HashMap::new();

    substitutions.insert("filename".to_string(), filename.to_string());
    substitutions.insert("ext".to_string(), extension.to_string());
    substitutions.insert(
        "filetype".to_string(),
        if asset.asset_type == "IMAGE" {
            "IMG".to_string()
        } else {
            "VID".to_string()
        },
    );
    substitutions.insert(
        "filetypefull".to_string(),
        if asset.asset_type == "IMAGE" {
            "IMAGE".to_string()
        } else {
            "VIDEO".to_string()
        },
    );
    substitutions.insert("assetId".to_string(), asset.id.to_string());
    substitutions.insert(
        "assetIdShort".to_string(),
        asset.id.to_string().chars().skip(24).collect(),
    );
    substitutions.insert(
        "album".to_string(),
        album_name.map(sanitize_album_name).unwrap_or_default(),
    );
    substitutions.insert("make".to_string(), asset.make.clone().unwrap_or_default());
    substitutions.insert("model".to_string(), asset.model.clone().unwrap_or_default());
    substitutions.insert(
        "lensModel".to_string(),
        asset.lens_model.clone().unwrap_or_default(),
    );

    for token in LUXON_TOKENS {
        substitutions.insert(
            (*token).to_string(),
            format_luxon_token(file_created_at, token),
        );
        if album_name.is_some() {
            substitutions.insert(
                format!("album-startDate-{token}"),
                album_start_date
                    .map(|dt| format_luxon_token(dt, token))
                    .unwrap_or_default(),
            );
            substitutions.insert(
                format!("album-endDate-{token}"),
                album_end_date
                    .map(|dt| format_luxon_token(dt, token))
                    .unwrap_or_default(),
            );
        }
    }

    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(true);
    let rendered = handlebars
        .render_template(template, &substitutions)
        .map_err(|err| err.to_string())?;
    Ok(rendered.replace("//", "/"))
}

async fn get_album_name(
    pool: &PgPool,
    owner_id: &Uuid,
    asset_id: &Uuid,
) -> Result<Option<String>, String> {
    sqlx::query_scalar(
        r#"
        SELECT album."albumName"
        FROM album
        INNER JOIN album_asset ON album.id = album_asset."albumId"
        WHERE album_asset."assetId" = $1
          AND album."ownerId" = $2
        LIMIT 1
        "#,
    )
    .bind(asset_id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await
    .map_err(|err| err.to_string())
}

async fn path_exists(path: &str) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

fn is_android_motion_path(storage: &StoragePaths, original_path: &str) -> bool {
    let base = normalize_path_string(&storage.encoded_video_base());
    normalize_path_string(Path::new(original_path)).starts_with(&base)
}

fn live_photo_motion_filename(still_name: &str, motion_path: &str) -> String {
    let still_stem = Path::new(still_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(still_name);
    let motion_ext = Path::new(motion_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if motion_ext.is_empty() {
        still_stem.to_string()
    } else {
        format!("{still_stem}.{motion_ext}")
    }
}

fn normalize_extension(extension: &str) -> String {
    match extension {
        "jpeg" | "jpe" => "jpg".to_string(),
        "tif" => "tiff".to_string(),
        "3gpp" => "3gp".to_string(),
        "mpeg" | "mpe" => "mpg".to_string(),
        "m2ts" | "m2t" => "mts".to_string(),
        other => other.to_string(),
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

fn sanitize_album_name(name: &str) -> String {
    sanitize_filename(&name.replace("..", ""))
}

fn normalize_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn join_library_path(root: &Path, rendered: &str) -> PathBuf {
    let relative = rendered.trim_matches('/').replace('/', std::path::MAIN_SEPARATOR_STR);
    root.join(relative)
}

fn format_luxon_token(dt: DateTime<Utc>, token: &str) -> String {
    match token {
        "y" => dt.format("%Y").to_string(),
        "yy" => dt.format("%y").to_string(),
        "M" => dt.month().to_string(),
        "MM" => format!("{:02}", dt.month()),
        "MMM" => dt.format("%b").to_string(),
        "MMMM" => dt.format("%B").to_string(),
        "d" => dt.day().to_string(),
        "dd" => format!("{:02}", dt.day()),
        "W" => dt.format("%U").to_string(),
        "WW" => dt.format("%U").to_string(),
        "H" => format!("{:02}", dt.hour()),
        "HH" => format!("{:02}", dt.hour()),
        "h" => {
            let hour = dt.hour();
            let hour12 = if hour % 12 == 0 { 12 } else { hour % 12 };
            hour12.to_string()
        }
        "hh" => {
            let hour = dt.hour();
            let hour12 = if hour % 12 == 0 { 12 } else { hour % 12 };
            format!("{hour12:02}")
        }
        "m" => dt.minute().to_string(),
        "mm" => format!("{:02}", dt.minute()),
        "s" => dt.second().to_string(),
        "ss" => format!("{:02}", dt.second()),
        "SSS" => format!("{:03}", dt.timestamp_subsec_millis()),
        other => other.to_string(),
    }
}
