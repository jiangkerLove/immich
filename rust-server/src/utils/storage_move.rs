use std::path::Path;

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::move_history::{self, MoveHistoryRow};
use crate::utils::checksum::sha1_bytes;
use crate::utils::storage::StoragePaths;

#[derive(Debug, Clone)]
pub struct MoveFileOptions {
    pub entity_id: Uuid,
    pub path_type: String,
    pub old_path: Option<String>,
    pub new_path: String,
    pub expected_size: Option<i64>,
    pub expected_checksum: Option<Vec<u8>>,
    pub hash_verification: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveFileOutcome {
    Completed,
    Skipped,
}

pub async fn move_file(pool: &PgPool, options: MoveFileOptions) -> Result<MoveFileOutcome, String> {
    let entity_id = options.entity_id;
    let path_type = options.path_type.clone();
    let new_path = options.new_path.clone();
    let expected_size = options.expected_size;
    let expected_checksum = options.expected_checksum.clone();
    let hash_verification = options.hash_verification;

    let Some(old_path) = options.old_path.filter(|path| !path.is_empty()) else {
        return Ok(MoveFileOutcome::Skipped);
    };

    if old_path == new_path {
        return Ok(MoveFileOutcome::Skipped);
    }

    if let Some(parent) = Path::new(&new_path).parent() {
        StoragePaths::ensure_parent(parent).map_err(|err| err.to_string())?;
    }

    let mut move_row = move_history::get_by_entity(pool, &entity_id, &path_type)
        .await
        .map_err(|err| err.to_string())?;

    if let Some(existing) = move_row.as_ref() {
        let old_exists = path_exists(&existing.old_path).await;
        let new_exists = path_exists(&existing.new_path).await;
        let actual_path = if old_exists {
            Some(existing.old_path.as_str())
        } else if new_exists {
            Some(existing.new_path.as_str())
        } else {
            None
        };

        let Some(actual_path) = actual_path else {
            eprintln!(
                "unable to complete move for {entity_id} ({path_type}): file missing at both locations"
            );
            return Ok(MoveFileOutcome::Skipped);
        };

        let file_at_new_location = actual_path == existing.new_path;
        if file_at_new_location
            && !verify_contents(
                &existing.old_path,
                &existing.new_path,
                expected_size,
                expected_checksum.as_deref(),
                hash_verification,
            )
            .await?
        {
            eprintln!(
                "skipping move for {entity_id} ({path_type}): verification failed at new location"
            );
            return Ok(MoveFileOutcome::Skipped);
        }

        if actual_path != existing.old_path || existing.new_path != new_path {
            move_history::update_paths(pool, &existing.id, actual_path, &new_path)
                .await
                .map_err(|err| err.to_string())?;
            move_row = move_history::get_by_entity(pool, &entity_id, &path_type)
                .await
                .map_err(|err| err.to_string())?;
        }
    } else {
        move_row = Some(
            move_history::create(pool, &entity_id, &path_type, &old_path, &new_path)
                .await
                .map_err(|err| err.to_string())?,
        );
    }

    let move_row = move_row.expect("move history row must exist");
    if move_row.old_path != new_path {
        if !perform_physical_move(
            &move_row,
            &new_path,
            entity_id,
            &path_type,
            expected_size,
            expected_checksum.as_deref(),
            hash_verification,
        )
        .await?
        {
            return Ok(MoveFileOutcome::Skipped);
        }
    }

    move_history::delete_by_id(pool, &move_row.id)
        .await
        .map_err(|err| err.to_string())?;

    Ok(MoveFileOutcome::Completed)
}

async fn perform_physical_move(
    move_row: &MoveHistoryRow,
    new_path: &str,
    entity_id: Uuid,
    path_type: &str,
    expected_size: Option<i64>,
    expected_checksum: Option<&[u8]>,
    hash_verification: bool,
) -> Result<bool, String> {
    match tokio::fs::rename(&move_row.old_path, new_path).await {
        Ok(()) => Ok(true),
        Err(err) if is_cross_device_error(&err) => {
            tokio::fs::copy(&move_row.old_path, new_path)
                .await
                .map_err(|err| err.to_string())?;

            if !verify_contents(
                &move_row.old_path,
                new_path,
                expected_size,
                expected_checksum,
                hash_verification,
            )
            .await?
            {
                eprintln!(
                    "skipping move for {entity_id} ({path_type}): verification failed after copy"
                );
                let _ = tokio::fs::remove_file(new_path).await;
                return Ok(false);
            }

            if let Err(err) = tokio::fs::remove_file(&move_row.old_path).await {
                eprintln!(
                    "unable to delete old file {} after copy: {err}",
                    move_row.old_path
                );
            }
            Ok(true)
        }
        Err(err) => {
            eprintln!(
                "unable to complete move for {entity_id} ({path_type}): rename failed: {err}"
            );
            Ok(false)
        }
    }
}

async fn verify_contents(
    old_path: &str,
    new_path: &str,
    expected_size: Option<i64>,
    expected_checksum: Option<&[u8]>,
    hash_verification: bool,
) -> Result<bool, String> {
    let new_meta = tokio::fs::metadata(new_path)
        .await
        .map_err(|err| err.to_string())?;
    let old_size = if let Some(size) = expected_size.filter(|size| *size > 0) {
        size
    } else if let Ok(old_meta) = tokio::fs::metadata(old_path).await {
        old_meta.len() as i64
    } else {
        new_meta.len() as i64
    };

    if new_meta.len() as i64 != old_size {
        return Ok(false);
    }

    if hash_verification {
        if let Some(expected_checksum) = expected_checksum.filter(|checksum| !checksum.is_empty()) {
            let new_checksum = sha1_file(new_path).await?;
            if new_checksum != expected_checksum {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

async fn sha1_file(path: &str) -> Result<Vec<u8>, String> {
    let bytes = tokio::fs::read(path).await.map_err(|err| err.to_string())?;
    Ok(sha1_bytes(&bytes))
}

async fn path_exists(path: &str) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

fn is_cross_device_error(err: &std::io::Error) -> bool {
    matches!(err.raw_os_error(), Some(18) | Some(17))
}
