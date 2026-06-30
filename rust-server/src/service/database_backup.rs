use std::path::Path;

use axum::body::Body;
use axum::http::Response;
use serde::{Deserialize, Serialize};

use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::utils::database_backups::is_valid_database_backup_name;
use crate::utils::file_response::{file_response, FileResponse};
use crate::utils::permission::require_admin;
use crate::utils::storage::StoragePaths;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseBackupItem {
    pub filename: String,
    pub filesize: i64,
    pub timezone: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseBackupListResponse {
    pub backups: Vec<DatabaseBackupItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseBackupDeleteReq {
    pub backups: Vec<String>,
}

#[derive(Clone)]
pub struct DatabaseBackupService {
    storage: StoragePaths,
}

impl DatabaseBackupService {
    pub fn new(storage: StoragePaths) -> Self {
        Self { storage }
    }

    fn backups_dir(&self) -> std::path::PathBuf {
        self.storage.backups_folder()
    }

    fn local_timezone_label() -> String {
        chrono::Local::now().offset().to_string()
    }

    pub async fn list_backups(&self, auth: &AuthDto) -> Result<DatabaseBackupListResponse, ErrorResp> {
        require_admin(auth)?;
        self.list_backups_internal().await
    }

    pub async fn list_backups_internal(&self) -> Result<DatabaseBackupListResponse, ErrorResp> {
        let dir = self.backups_dir();
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?;

        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?;

        let mut filenames = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?
        {
            if entry
                .file_type()
                .await
                .map(|file_type| file_type.is_file())
                .unwrap_or(false)
            {
                if let Some(name) = entry.file_name().to_str() {
                    if is_valid_database_backup_name(name) {
                        filenames.push(name.to_string());
                    }
                }
            }
        }

        filenames.sort_by(|a, b| {
            let a_uploaded = a.starts_with("uploaded-");
            let b_uploaded = b.starts_with("uploaded-");
            if a_uploaded == b_uploaded {
                a.cmp(b)
            } else if a_uploaded {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            }
        });
        filenames.reverse();

        let timezone = Self::local_timezone_label();
        let mut backups = Vec::with_capacity(filenames.len());
        for filename in filenames {
            let metadata = tokio::fs::metadata(dir.join(&filename))
                .await
                .map_err(|err| ErrorResp::ServerError(err.to_string()))?;
            backups.push(DatabaseBackupItem {
                filename,
                filesize: metadata.len() as i64,
                timezone: timezone.clone(),
            });
        }

        Ok(DatabaseBackupListResponse { backups })
    }

    pub async fn download_backup(
        &self,
        auth: &AuthDto,
        filename: &str,
    ) -> Result<Response<Body>, ErrorResp> {
        require_admin(auth)?;
        self.download_backup_internal(filename).await
    }

    pub async fn download_backup_internal(
        &self,
        filename: &str,
    ) -> Result<Response<Body>, ErrorResp> {
        if !is_valid_database_backup_name(filename) {
            return Err(ErrorResp::BadRequest("Invalid backup name!".to_string()));
        }

        let path = self.backups_dir().join(filename);
        if !path.exists() {
            return Err(ErrorResp::BadRequest("Backup not found".to_string()));
        }

        let content_type = if filename.ends_with(".gz") {
            "application/gzip"
        } else {
            "application/sql"
        };

        file_response(FileResponse {
            path: path.to_string_lossy().to_string(),
            content_type: content_type.to_string(),
            file_name: Some(filename.to_string()),
            cache_control: Some("private, no-cache, no-transform".to_string()),
        })
        .await
    }

    pub async fn delete_backups(
        &self,
        auth: &AuthDto,
        filenames: &[String],
    ) -> Result<(), ErrorResp> {
        require_admin(auth)?;
        self.delete_backups_internal(filenames).await
    }

    pub async fn delete_backups_internal(&self, filenames: &[String]) -> Result<(), ErrorResp> {
        if filenames
            .iter()
            .any(|filename| !is_valid_database_backup_name(filename))
        {
            return Err(ErrorResp::BadRequest("Invalid backup name!".to_string()));
        }

        for filename in filenames {
            let path = self.backups_dir().join(filename);
            if path.exists() {
                tokio::fs::remove_file(&path)
                    .await
                    .map_err(|err| ErrorResp::ServerError(err.to_string()))?;
            }
        }

        Ok(())
    }

    pub async fn upload_backup(
        &self,
        auth: &AuthDto,
        original_name: &str,
        bytes: Vec<u8>,
    ) -> Result<(), ErrorResp> {
        require_admin(auth)?;
        self.upload_backup_internal(original_name, bytes).await
    }

    pub async fn upload_backup_internal(
        &self,
        original_name: &str,
        bytes: Vec<u8>,
    ) -> Result<(), ErrorResp> {
        let base_name = Path::new(original_name)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(original_name);

        if !is_valid_database_backup_name(base_name) {
            return Err(ErrorResp::BadRequest("Invalid backup name!".to_string()));
        }

        let dir = self.backups_dir();
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?;

        let path = dir.join(format!("uploaded-{base_name}"));
        tokio::fs::write(path, bytes)
            .await
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?;
        Ok(())
    }
}
