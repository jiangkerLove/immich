use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::library::{self, LibraryRow};
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::job::JobService;
use crate::utils::permission::require_admin;

const DEFAULT_EXCLUSIONS: &[&str] = &[
    "**/@eaDir/**",
    "**/._*",
    "**/#recycle/**",
    "**/#snapshot/**",
    "**/.stversions/**",
    "**/.stfolder/**",
];

#[derive(Clone)]
pub struct LibraryService {
    pool: PgPool,
    jobs: JobService,
    media_location: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryResponse {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub asset_count: i64,
    pub import_paths: Vec<String>,
    pub exclusion_patterns: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub refreshed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStatsResponse {
    pub photos: i64,
    pub videos: i64,
    pub total: i64,
    pub usage: i64,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLibraryReq {
    pub owner_id: Uuid,
    pub name: Option<String>,
    pub import_paths: Option<Vec<String>>,
    pub exclusion_patterns: Option<Vec<String>>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLibraryReq {
    pub name: Option<String>,
    pub import_paths: Option<Vec<String>>,
    pub exclusion_patterns: Option<Vec<String>>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateLibraryReq {
    pub import_paths: Option<Vec<String>>,
    pub exclusion_patterns: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateLibraryImportPathResponse {
    pub import_path: String,
    pub is_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateLibraryResponse {
    pub import_paths: Vec<ValidateLibraryImportPathResponse>,
}

impl LibraryService {
    pub fn new(pool: PgPool, jobs: JobService, media_location: PathBuf) -> Self {
        Self {
            pool,
            jobs,
            media_location,
        }
    }

    pub async fn get_all(&self, auth: &AuthDto) -> Result<Vec<LibraryResponse>, ErrorResp> {
        require_admin(auth)?;
        let rows = library::list_all(&self.pool).await?;
        Ok(rows.into_iter().map(map_library).collect())
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<LibraryResponse, ErrorResp> {
        require_admin(auth)?;
        self.find_or_fail(id).await.map(map_library)
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &CreateLibraryReq,
    ) -> Result<LibraryResponse, ErrorResp> {
        require_admin(auth)?;
        let name = dto.name.as_deref().unwrap_or("New External Library");
        let import_paths = dto.import_paths.clone().unwrap_or_default();
        let exclusion_patterns = dto
            .exclusion_patterns
            .clone()
            .unwrap_or_else(default_exclusions);

        let row = library::create(
            &self.pool,
            &dto.owner_id,
            name,
            &import_paths,
            &exclusion_patterns,
        )
        .await?;
        Ok(map_library(row))
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &UpdateLibraryReq,
    ) -> Result<LibraryResponse, ErrorResp> {
        require_admin(auth)?;
        self.find_or_fail(id).await?;

        if let Some(import_paths) = &dto.import_paths {
            let validation = self.validate_paths(import_paths).await?;
            for path in validation {
                if !path.is_valid {
                    return Err(ErrorResp::BadRequest(format!(
                        "Invalid import path: {}",
                        path.message.unwrap_or_default()
                    )));
                }
            }
        }

        let row = library::update(
            &self.pool,
            id,
            dto.name.as_deref(),
            dto.import_paths.as_deref(),
            dto.exclusion_patterns.as_deref(),
        )
        .await?;
        Ok(map_library(row))
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_admin(auth)?;
        self.find_or_fail(id).await?;
        library::soft_delete(&self.pool, id).await?;
        self.jobs.queue_library_delete(id).await
    }

    pub async fn validate(
        &self,
        auth: &AuthDto,
        dto: &ValidateLibraryReq,
    ) -> Result<ValidateLibraryResponse, ErrorResp> {
        require_admin(auth)?;
        let import_paths = self.validate_paths(dto.import_paths.as_deref().unwrap_or(&[])).await?;
        Ok(ValidateLibraryResponse { import_paths })
    }

    pub async fn get_statistics(
        &self,
        auth: &AuthDto,
        id: &Uuid,
    ) -> Result<LibraryStatsResponse, ErrorResp> {
        require_admin(auth)?;
        self.find_or_fail(id).await?;

        let stats = library::get_statistics(&self.pool, id)
            .await?
            .unwrap_or(library::LibraryStatsRow {
                photos: 0,
                videos: 0,
                usage: 0,
            });

        Ok(LibraryStatsResponse {
            photos: stats.photos,
            videos: stats.videos,
            total: stats.photos + stats.videos,
            usage: stats.usage,
        })
    }

    pub async fn queue_scan(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_admin(auth)?;
        self.find_or_fail(id).await?;
        self.jobs.queue_library_scan(id).await
    }

    async fn find_or_fail(&self, id: &Uuid) -> Result<LibraryRow, ErrorResp> {
        library::get_by_id(&self.pool, id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest(format!("Library {id} not found")))
    }

    async fn validate_paths(
        &self,
        paths: &[String],
    ) -> Result<Vec<ValidateLibraryImportPathResponse>, ErrorResp> {
        let mut results = Vec::with_capacity(paths.len());
        for import_path in paths {
            results.push(self.validate_import_path(import_path).await);
        }
        Ok(results)
    }

    async fn validate_import_path(&self, import_path: &str) -> ValidateLibraryImportPathResponse {
        if is_under_media_location(import_path, &self.media_location) {
            return ValidateLibraryImportPathResponse {
                import_path: import_path.to_string(),
                is_valid: false,
                message: Some("Cannot use media upload folder for external libraries".to_string()),
            };
        }

        let path = Path::new(import_path);
        if !path.is_absolute() {
            return ValidateLibraryImportPathResponse {
                import_path: import_path.to_string(),
                is_valid: false,
                message: Some(format!(
                    "Import path must be absolute, try {}",
                    std::env::current_dir()
                        .ok()
                        .map(|cwd| cwd.join(import_path).to_string_lossy().to_string())
                        .unwrap_or_else(|| import_path.to_string())
                )),
            };
        }

        match tokio::fs::metadata(import_path).await {
            Ok(metadata) if metadata.is_dir() => ValidateLibraryImportPathResponse {
                import_path: import_path.to_string(),
                is_valid: true,
                message: None,
            },
            Ok(_) => ValidateLibraryImportPathResponse {
                import_path: import_path.to_string(),
                is_valid: false,
                message: Some("Not a directory".to_string()),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                ValidateLibraryImportPathResponse {
                    import_path: import_path.to_string(),
                    is_valid: false,
                    message: Some("Path does not exist (ENOENT)".to_string()),
                }
            }
            Err(err) => ValidateLibraryImportPathResponse {
                import_path: import_path.to_string(),
                is_valid: false,
                message: Some(err.to_string()),
            },
        }
    }
}

fn map_library(row: LibraryRow) -> LibraryResponse {
    LibraryResponse {
        id: row.id,
        owner_id: row.owner_id,
        name: row.name,
        asset_count: row.asset_count,
        import_paths: row.import_paths,
        exclusion_patterns: row.exclusion_patterns,
        created_at: row.created_at,
        updated_at: row.updated_at,
        refreshed_at: row.refreshed_at,
    }
}

fn default_exclusions() -> Vec<String> {
    DEFAULT_EXCLUSIONS.iter().map(|value| value.to_string()).collect()
}

fn is_under_media_location(path: &str, media_location: &Path) -> bool {
    let Ok(canonical_media) = media_location.canonicalize() else {
        return path.starts_with(media_location.to_string_lossy().as_ref());
    };
    let Ok(canonical_path) = Path::new(path).canonicalize() else {
        return path.starts_with(media_location.to_string_lossy().as_ref());
    };
    canonical_path.starts_with(canonical_media)
}
