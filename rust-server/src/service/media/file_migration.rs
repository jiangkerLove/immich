use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::asset_job::{self, UpsertAssetFile};
use crate::models::db::migration_job::{self, find_asset_file, parse_asset_files, MigrationAssetRow};
use crate::models::db::system_metadata::get_json;
use crate::service::job::JobService;
use crate::utils::storage::StoragePaths;
use crate::utils::storage_move::{MoveFileOptions, MoveFileOutcome, move_file};

const JOBS_BATCH_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMigrationOutcome {
    Skipped,
    Failed,
    Success,
}

#[derive(Debug, Clone)]
struct ImageFormatConfig {
    preview_format: String,
    thumbnail_format: String,
    fullsize_format: String,
}

impl Default for ImageFormatConfig {
    fn default() -> Self {
        Self {
            preview_format: "jpeg".into(),
            thumbnail_format: "webp".into(),
            fullsize_format: "jpeg".into(),
        }
    }
}

#[derive(Clone)]
pub struct FileMigrationService {
    pool: PgPool,
    storage: StoragePaths,
    jobs: JobService,
}

impl FileMigrationService {
    pub fn new(pool: PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self {
            pool,
            storage,
            jobs,
        }
    }

    pub async fn queue_all(&self) -> Result<(), String> {
        StoragePaths::remove_empty_dirs(&self.storage.thumbs_base(), false)
            .await?;
        StoragePaths::remove_empty_dirs(&self.storage.encoded_video_base(), false)
            .await?;

        let asset_ids = migration_job::stream_for_migration(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        for chunk in asset_ids.chunks(JOBS_BATCH_SIZE) {
            for asset_id in chunk {
                self.jobs
                    .queue_asset_file_migration(asset_id)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        let person_ids = migration_job::stream_persons_for_migration(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        for chunk in person_ids.chunks(JOBS_BATCH_SIZE) {
            for person_id in chunk {
                self.jobs
                    .queue_person_file_migration(person_id)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        Ok(())
    }

    pub async fn migrate_asset(&self, asset_id: &Uuid) -> Result<FileMigrationOutcome, String> {
        let Some(asset) = migration_job::get_for_migration(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(FileMigrationOutcome::Failed);
        };

        let config = self.load_image_config().await?;
        let files = parse_asset_files(asset.files.clone());

        self.move_asset_image(&asset, &files, "fullsize", &config.fullsize_format)
            .await?;
        self.move_asset_image(&asset, &files, "preview", &config.preview_format)
            .await?;
        self.move_asset_image(&asset, &files, "thumbnail", &config.thumbnail_format)
            .await?;
        self.move_asset_video(&asset, &files).await?;

        Ok(FileMigrationOutcome::Success)
    }

    pub async fn migrate_person(&self, person_id: &Uuid) -> Result<FileMigrationOutcome, String> {
        let Some(person) = migration_job::get_person_for_migration(&self.pool, person_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(FileMigrationOutcome::Failed);
        };

        if person.thumbnail_path.is_empty() {
            return Ok(FileMigrationOutcome::Skipped);
        }

        let new_path = self
            .storage
            .person_thumbnail_path(&person.owner_id, person_id);
        self.run_move(
            *person_id,
            "face",
            Some(person.thumbnail_path.clone()),
            new_path.to_string_lossy().into_owned(),
            SavePathTarget::PersonThumbnail(*person_id),
        )
        .await?;

        Ok(FileMigrationOutcome::Success)
    }

    async fn move_asset_image(
        &self,
        asset: &MigrationAssetRow,
        files: &[crate::models::db::asset_job::AssetFileJobRow],
        file_type: &str,
        format: &str,
    ) -> Result<(), String> {
        let old_file = find_asset_file(files, file_type, false);
        let new_path = self.storage.image_derivative_path(
            &asset.owner_id,
            &asset.id,
            file_type,
            format,
            false,
        );
        self.run_move(
            asset.id,
            file_type,
            old_file.map(|file| file.path.clone()),
            new_path.to_string_lossy().into_owned(),
            SavePathTarget::AssetFile {
                asset_id: asset.id,
                file_type: file_type.to_string(),
                is_edited: false,
                is_progressive: old_file.map(|file| file.is_progressive).unwrap_or(false),
                is_transparent: old_file.map(|file| file.is_transparent).unwrap_or(false),
            },
        )
        .await
    }

    async fn move_asset_video(
        &self,
        asset: &MigrationAssetRow,
        files: &[crate::models::db::asset_job::AssetFileJobRow],
    ) -> Result<(), String> {
        let old_file = find_asset_file(files, "encoded_video", false);
        let new_path = self
            .storage
            .encoded_video_path(&asset.owner_id, &asset.id);
        self.run_move(
            asset.id,
            "encoded_video",
            old_file.map(|file| file.path.clone()),
            new_path.to_string_lossy().into_owned(),
            SavePathTarget::AssetFile {
                asset_id: asset.id,
                file_type: "encoded_video".into(),
                is_edited: false,
                is_progressive: false,
                is_transparent: false,
            },
        )
        .await
    }

    async fn run_move(
        &self,
        entity_id: Uuid,
        path_type: &str,
        old_path: Option<String>,
        new_path: String,
        save_target: SavePathTarget,
    ) -> Result<(), String> {
        let outcome = move_file(
            &self.pool,
            MoveFileOptions {
                entity_id,
                path_type: path_type.to_string(),
                old_path,
                new_path: new_path.clone(),
                expected_size: None,
                expected_checksum: None,
                hash_verification: false,
            },
        )
        .await?;

        if outcome == MoveFileOutcome::Completed {
            save_path(&self.pool, save_target, new_path).await?;
        }

        Ok(())
    }

    async fn load_image_config(&self) -> Result<ImageFormatConfig, String> {
        let mut config = ImageFormatConfig::default();
        let stored = get_json(&self.pool, "system-config")
            .await
            .map_err(|err| err.to_string())?;
        if let Some(image) = stored.and_then(|value| value.get("image").cloned()) {
            if let Some(preview) = image.get("preview") {
                config.preview_format = read_string(preview, "format", &config.preview_format);
            }
            if let Some(thumbnail) = image.get("thumbnail") {
                config.thumbnail_format =
                    read_string(thumbnail, "format", &config.thumbnail_format);
            }
            if let Some(fullsize) = image.get("fullsize") {
                config.fullsize_format =
                    read_string(fullsize, "format", &config.fullsize_format);
            }
        }
        Ok(config)
    }
}

#[derive(Debug, Clone)]
enum SavePathTarget {
    AssetFile {
        asset_id: Uuid,
        file_type: String,
        is_edited: bool,
        is_progressive: bool,
        is_transparent: bool,
    },
    PersonThumbnail(Uuid),
}

async fn save_path(pool: &PgPool, target: SavePathTarget, new_path: String) -> Result<(), String> {
    match target {
        SavePathTarget::AssetFile {
            asset_id,
            file_type,
            is_edited,
            is_progressive,
            is_transparent,
        } => {
            asset_job::upsert_asset_files(
                pool,
                &[UpsertAssetFile {
                    asset_id,
                    path: new_path,
                    file_type,
                    is_edited,
                    is_progressive,
                    is_transparent,
                }],
            )
            .await
            .map_err(|err| err.to_string())?;
        }
        SavePathTarget::PersonThumbnail(person_id) => {
            asset_job::update_person_thumbnail_path(pool, &person_id, &new_path)
                .await
                .map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn read_string(value: &serde_json::Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}
