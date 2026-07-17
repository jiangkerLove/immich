use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::PgPool;
use tokio::io::AsyncWriteExt;

use crate::models::db::advisory_lock::{self, LOCK_MEDIA_LOCATION, LOCK_SYSTEM_FILE_MOUNTS};
use crate::models::db::media_location;
use crate::models::db::system_metadata::{self, MediaLocationMeta, SystemFlags};
use crate::models::dto::env::EnvDto;
use crate::utils::storage::StoragePaths;

const DOCS_MESSAGE: &str =
    "Please see https://docs.immich.app/administration/system-integrity#folder-checks for more information.";

const INCONSISTENT_MEDIA_LOCATION: &str = "Detected an inconsistent media location. For more information, see https://docs.immich.app/errors#inconsistent-media-location";

const STORAGE_FOLDERS: &[&str] = &[
    "encoded-video",
    "library",
    "upload",
    "profile",
    "thumbs",
    "backups",
];

pub fn detect_media_location(settings: &EnvDto) -> PathBuf {
    if let Some(location) = settings
        .immich_media_location
        .as_ref()
        .or(settings.upload_location.as_ref())
        .filter(|value| !value.is_empty())
    {
        return PathBuf::from(location);
    }

    let candidates = ["/data", "/usr/src/app/upload"];
    let mut found = Vec::new();
    for candidate in candidates {
        if Path::new(candidate).exists() {
            found.push(candidate);
        }
    }

    if found.len() == 1 {
        return PathBuf::from(found[0]);
    }

    if Path::new("/usr/src/app/upload").exists() {
        return PathBuf::from("/usr/src/app/upload");
    }

    PathBuf::from("./library")
}

pub async fn on_bootstrap(
    pool: &PgPool,
    settings: &EnvDto,
    storage: &StoragePaths,
) -> Result<(), String> {
    verify_mounts(pool, settings, storage).await?;
    sync_media_location(pool, settings, storage).await?;
    Ok(())
}

async fn verify_mounts(
    pool: &PgPool,
    settings: &EnvDto,
    storage: &StoragePaths,
) -> Result<(), String> {
    let result = advisory_lock::run_with_lock(pool, LOCK_SYSTEM_FILE_MOUNTS, || async {
        let mut flags = system_metadata::get_system_flags(pool)
            .await
            .map_err(|err| err.to_string())?
            .unwrap_or_default();

        println!(
            "storage bootstrap: verifying mount folder checks, current state: {}",
            serde_json::to_string(&flags).unwrap_or_else(|_| "{}".into())
        );

        let check_result = run_mount_checks(storage, &mut flags).await;
        match check_result {
            Ok(updated) => {
                if updated {
                    system_metadata::set_system_flags(pool, &flags)
                        .await
                        .map_err(|err| err.to_string())?;
                    println!("storage bootstrap: successfully enabled system mount folder checks");
                }
                println!("storage bootstrap: successfully verified system mount folder checks");
                Ok(())
            }
            Err(err) => {
                if settings.immich_ignore_mount_check_errors.unwrap_or(false) {
                    eprintln!("storage bootstrap: {err}");
                    eprintln!("storage bootstrap: ignoring mount folder errors");
                    Ok(())
                } else {
                    Err(err)
                }
            }
        }
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(err) => Err(err.to_string()),
    }
}

async fn run_mount_checks(storage: &StoragePaths, flags: &mut SystemFlags) -> Result<bool, String> {
    let mut updated = false;

    for folder in STORAGE_FOLDERS {
        let already_checked = flags.mount_checks.get(*folder).copied().unwrap_or(false);
        if !already_checked {
            println!("storage bootstrap: writing initial mount file for the {folder} folder");
            create_mount_file(storage, folder).await?;
        }

        verify_read_access(storage, folder).await?;
        verify_write_access(storage, folder).await?;

        if !already_checked {
            flags.mount_checks.insert((*folder).to_string(), true);
            updated = true;
        }
    }

    Ok(updated)
}

async fn create_mount_file(storage: &StoragePaths, folder: &str) -> Result<(), String> {
    let folder_path = storage.media_location().join(folder);
    let internal_path = folder_path.join(".immich");
    let external_path = format!("<UPLOAD_LOCATION>/{folder}/.immich");

    tokio::fs::create_dir_all(&folder_path)
        .await
        .map_err(|err| {
            format!("Failed to create \"{external_path} - {DOCS_MESSAGE}\" ({err})")
        })?;

    match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&internal_path)
        .await
    {
        Ok(mut file) => {
            let contents = now_millis();
            file.write_all(contents.as_bytes())
                .await
                .map_err(|err| format!("Failed to create \"{external_path} - {DOCS_MESSAGE}\" ({err})"))?;
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            eprintln!("storage bootstrap: found existing mount file, skipping creation");
            Ok(())
        }
        Err(err) => Err(format!(
            "Failed to create \"{external_path} - {DOCS_MESSAGE}\" ({err})"
        )),
    }
}

async fn verify_read_access(storage: &StoragePaths, folder: &str) -> Result<(), String> {
    let internal_path = storage.media_location().join(folder).join(".immich");
    let external_path = format!("<UPLOAD_LOCATION>/{folder}/.immich");
    tokio::fs::read(&internal_path).await.map_err(|err| {
        eprintln!("storage bootstrap: failed to read ({}): {err}", internal_path.display());
        format!(
            "Failed to read: \"{external_path} ({}) - {DOCS_MESSAGE}\"",
            internal_path.display()
        )
    })?;
    Ok(())
}

async fn verify_write_access(storage: &StoragePaths, folder: &str) -> Result<(), String> {
    let internal_path = storage.media_location().join(folder).join(".immich");
    let external_path = format!("<UPLOAD_LOCATION>/{folder}/.immich");
    tokio::fs::write(&internal_path, now_millis().as_bytes())
        .await
        .map_err(|err| {
            eprintln!(
                "storage bootstrap: failed to write {}: {err}",
                internal_path.display()
            );
            format!("Failed to write \"{external_path} - {DOCS_MESSAGE}\"")
        })?;
    Ok(())
}

async fn sync_media_location(
    pool: &PgPool,
    settings: &EnvDto,
    storage: &StoragePaths,
) -> Result<(), String> {
    let result = advisory_lock::run_with_lock(pool, LOCK_MEDIA_LOCATION, || async {
        let current = normalize_location(&storage.media_location().to_string_lossy());
        let samples = media_location::sample_file_paths(pool)
            .await
            .map_err(|err| err.to_string())?;
        let saved = system_metadata::get_media_location(pool)
            .await
            .map_err(|err| err.to_string())?;

        if let Some(path) = samples.first() {
            let mut previous = saved
                .as_ref()
                .map(|value| value.location.clone())
                .unwrap_or_default();

            if previous.is_empty()
                && settings
                    .immich_media_location
                    .as_ref()
                    .or(settings.upload_location.as_ref())
                    .is_some()
            {
                previous = current.clone();
            }

            if previous.is_empty() {
                previous = if path.starts_with("upload/") {
                    "upload".to_string()
                } else {
                    "/usr/src/app/upload".to_string()
                };
            }

            let previous = normalize_location(&previous);
            if previous != current {
                println!("storage bootstrap: media location changed (from={previous}, to={current})");
                if !path.starts_with(&previous) {
                    return Err(INCONSISTENT_MEDIA_LOCATION.to_string());
                }

                eprintln!(
                    "storage bootstrap: detected a change to media location, performing an automatic migration of file paths from {previous} to {current}"
                );
                let updated = media_location::migrate_file_paths(pool, &previous, &current)
                    .await
                    .map_err(|err| err.to_string())?;
                println!("storage bootstrap: migrated {updated} path rows");
            }
        }

        if saved.as_ref().map(|value| value.location.as_str()) != Some(current.as_str()) {
            system_metadata::set_media_location(
                pool,
                &MediaLocationMeta {
                    location: current.clone(),
                },
            )
            .await
            .map_err(|err| err.to_string())?;
            println!("storage bootstrap: saved MediaLocation={current}");
        }

        Ok(())
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(err) => Err(err.to_string()),
    }
}

fn normalize_location(path: &str) -> String {
    path.trim_end_matches('/').to_string()
}

fn now_millis() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
