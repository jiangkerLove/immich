use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::assets::{self, NewAsset};
use crate::models::db::shared_links;
use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::access::{require_asset_access, require_upload_access};
use crate::service::job::JobService;
use crate::utils::checksum::{decode_checksum, sha1_bytes};
use crate::utils::file_response::{file_extension, file_response, file_stem, guess_mime, FileResponse};
use crate::utils::storage::StoragePaths;

#[derive(Clone)]
pub struct AssetMediaService {
    pool: PgPool,
    storage: StoragePaths,
    jobs: JobService,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMediaResponse {
    pub id: Uuid,
    pub status: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMediaCreateReq {
    pub file_created_at: DateTime<Utc>,
    pub file_modified_at: DateTime<Utc>,
    pub filename: Option<String>,
    pub is_favorite: Option<bool>,
    pub duration: Option<i32>,
    pub live_photo_video_id: Option<Uuid>,
    pub visibility: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct BulkUploadCheckItem {
    pub id: String,
    pub checksum: String,
}

#[derive(serde::Deserialize)]
pub struct BulkUploadCheckReq {
    pub assets: Vec<BulkUploadCheckItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkUploadCheckResult {
    pub id: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_trashed: Option<bool>,
}

#[derive(Serialize)]
pub struct BulkUploadCheckResponse {
    pub results: Vec<BulkUploadCheckResult>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AssetMediaOptionsQuery {
    pub size: Option<String>,
    pub edited: Option<bool>,
}

impl AssetMediaService {
    pub fn new(pool: PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self { pool, storage, jobs }
    }

    pub async fn upload_asset(
        &self,
        auth: &AuthDto,
        dto: &AssetMediaCreateReq,
        file_bytes: &[u8],
        original_name: &str,
    ) -> Result<AssetMediaResponse, ErrorResp> {
        require_upload_access(auth)?;
        require_permission(auth, Permission::AssetUpload)?;

        if let Some(quota) = auth.user.quota_size_in_bytes {
            if auth.user.quota_usage_in_bytes + file_bytes.len() as i64 > quota {
                return Err(ErrorResp::BadRequest("Quota has been exceeded!".to_string()));
            }
        }

        let checksum = sha1_bytes(file_bytes);
        if let Some(existing) =
            assets::get_upload_id_by_checksum(&self.pool, &auth.user.id, &checksum).await?
        {
            self.attach_to_shared_link(auth, existing).await?;
            return Ok(AssetMediaResponse {
                id: existing,
                status: "duplicate".to_string(),
            });
        }

        let file_uuid = Uuid::new_v4().to_string().replace('-', "");
        let ext = file_extension(original_name);
        let stored_name = format!("{file_uuid}{ext}");
        let upload_dir = self.storage.upload_folder(&auth.user.id, &file_uuid);
        tokio::fs::create_dir_all(&upload_dir)
            .await
            .map_err(|e| ErrorResp::ServerError(e.to_string()))?;
        let upload_path = self.storage.upload_path(&auth.user.id, &file_uuid, &stored_name);
        tokio::fs::write(&upload_path, file_bytes)
            .await
            .map_err(|e| ErrorResp::ServerError(e.to_string()))?;

        let asset_type = guess_asset_type(&stored_name);
        let visibility = dto.visibility.as_deref().unwrap_or("timeline");

        let asset_id = match assets::create_asset(
            &self.pool,
            NewAsset {
                owner_id: auth.user.id,
                asset_type,
                original_path: upload_path.to_string_lossy().as_ref(),
                checksum: &checksum,
                file_created_at: dto.file_created_at,
                file_modified_at: dto.file_modified_at,
                is_favorite: dto.is_favorite.unwrap_or(false),
                duration: dto.duration,
                original_file_name: dto.filename.as_deref().unwrap_or(original_name),
                live_photo_video_id: dto.live_photo_video_id,
                visibility,
            },
        )
        .await
        {
            Ok(id) => id,
            Err(err) => {
                let _ = tokio::fs::remove_file(&upload_path).await;
                let _ = self
                    .jobs
                    .queue_file_delete(&[upload_path.to_string_lossy()])
                    .await;
                if is_duplicate_error(&err) {
                    if let Some(existing) =
                        assets::get_upload_id_by_checksum(&self.pool, &auth.user.id, &checksum)
                            .await?
                    {
                        self.attach_to_shared_link(auth, existing).await?;
                        return Ok(AssetMediaResponse {
                            id: existing,
                            status: "duplicate".to_string(),
                        });
                    }
                }
                return Err(ErrorResp::from(err));
            }
        };

        assets::upsert_exif_size(&self.pool, &asset_id, file_bytes.len() as i64).await?;
        assets::update_quota_usage(&self.pool, &auth.user.id, file_bytes.len() as i64).await?;
        self.attach_to_shared_link(auth, asset_id).await?;

        self.jobs
            .queue_asset_extract_metadata_with_source(&asset_id, "upload")
            .await?;

        let _ = crate::service::workflow_trigger::on_asset_trigger(
            &self.pool,
            &self.jobs,
            &auth.user.id,
            &asset_id,
            crate::utils::workflow::TRIGGER_ASSET_CREATE,
        )
        .await;

        Ok(AssetMediaResponse {
            id: asset_id,
            status: "created".to_string(),
        })
    }

    pub async fn download_original(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
        edited: bool,
    ) -> Result<axum::http::Response<axum::body::Body>, ErrorResp> {
        require_asset_access(&self.pool, auth, asset_id, Permission::AssetDownload).await?;
        let use_edited = edited || auth.shared_link.is_some();
        let row = assets::get_for_original(&self.pool, asset_id, use_edited)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Asset not found".to_string()))?;

        let path = row.edited_path.unwrap_or(row.original_path);
        let file_name = format!(
            "{}{}",
            file_stem(&row.original_file_name),
            file_extension(&path)
        );

        file_response(FileResponse {
            path,
            content_type: guess_mime(&file_name),
            file_name: Some(file_name),
            cache_control: None,
        })
        .await
    }

    pub async fn view_thumbnail(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
        query: &AssetMediaOptionsQuery,
    ) -> Result<axum::http::Response<axum::body::Body>, ErrorResp> {
        require_asset_access(&self.pool, auth, asset_id, Permission::AssetView).await?;

        if query.size.as_deref() == Some("original") {
            return Err(ErrorResp::BadRequest("May not request original file".to_string()));
        }

        let file_type = match query.size.as_deref() {
            Some("preview") => "preview",
            Some("fullsize") => "fullsize",
            _ => "thumbnail",
        };

        let row = assets::get_for_thumbnail(&self.pool, asset_id, file_type)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Asset not found".to_string()))?;

        let path = row
            .path
            .ok_or_else(|| ErrorResp::BadRequest("Asset media not found".to_string()))?;

        let suffix = if auth.shared_link.is_some() && !auth.shared_link.as_ref().unwrap().show_exif {
            asset_id.to_string()
        } else {
            file_stem(&row.original_file_name)
        };
        let file_name = format!("{suffix}_{file_type}{}", file_extension(&path));

        file_response(FileResponse {
            path,
            content_type: guess_mime(&file_name),
            file_name: Some(file_name),
            cache_control: None,
        })
        .await
    }

    pub async fn playback_video(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
    ) -> Result<axum::http::Response<axum::body::Body>, ErrorResp> {
        require_asset_access(&self.pool, auth, asset_id, Permission::AssetView).await?;
        let row = assets::get_for_video(&self.pool, asset_id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Asset not found or asset is not a video".to_string()))?;

        let path = row.encoded_video_path.unwrap_or(row.original_path);
        file_response(FileResponse {
            path: path.clone(),
            content_type: guess_mime(&path),
            file_name: None,
            cache_control: None,
        })
        .await
    }

    pub async fn bulk_upload_check(
        &self,
        auth: &AuthDto,
        dto: &BulkUploadCheckReq,
    ) -> Result<BulkUploadCheckResponse, ErrorResp> {
        require_permission(auth, Permission::AssetUpload)?;

        let checksums: Result<Vec<Vec<u8>>, _> = dto
            .assets
            .iter()
            .map(|item| decode_checksum(&item.checksum))
            .collect();
        let checksums = checksums.map_err(|e| ErrorResp::BadRequest(e))?;

        let rows = assets::get_by_checksums(&self.pool, &auth.user.id, &checksums).await?;
        let mut map: HashMap<Vec<u8>, (Uuid, bool)> = HashMap::new();
        for row in rows {
            map.insert(
                row.checksum,
                (row.id, row.deleted_at.is_some()),
            );
        }

        let results = dto
            .assets
            .iter()
            .zip(checksums.iter())
            .map(|(item, checksum)| {
                if let Some((asset_id, is_trashed)) = map.get(checksum) {
                    BulkUploadCheckResult {
                        id: item.id.clone(),
                        action: "reject".to_string(),
                        reason: Some("duplicate".to_string()),
                        asset_id: Some(*asset_id),
                        is_trashed: Some(*is_trashed),
                    }
                } else {
                    BulkUploadCheckResult {
                        id: item.id.clone(),
                        action: "accept".to_string(),
                        reason: None,
                        asset_id: None,
                        is_trashed: None,
                    }
                }
            })
            .collect();

        Ok(BulkUploadCheckResponse { results })
    }

    async fn attach_to_shared_link(&self, auth: &AuthDto, asset_id: Uuid) -> Result<(), ErrorResp> {
        let Some(shared_link) = &auth.shared_link else {
            return Ok(());
        };
        let link_id = Uuid::parse_str(&shared_link.id)
            .map_err(|_| ErrorResp::ServerError("Invalid shared link".to_string()))?;

        if let Some(album_id) = &shared_link.album_id {
            let album_uuid = Uuid::parse_str(album_id)
                .map_err(|_| ErrorResp::ServerError("Invalid album".to_string()))?;
            shared_links::add_album_assets(&self.pool, &album_uuid, &[asset_id]).await?;
        } else {
            shared_links::add_assets(&self.pool, &link_id, &[asset_id]).await?;
        }
        Ok(())
    }
}

use crate::utils::permission::require_permission;

fn guess_asset_type(filename: &str) -> &'static str {
    let lower = filename.to_lowercase();
    if lower.ends_with(".mp4")
        || lower.ends_with(".mov")
        || lower.ends_with(".webm")
        || lower.ends_with(".mkv")
    {
        "VIDEO"
    } else if lower.ends_with(".mp3") || lower.ends_with(".wav") || lower.ends_with(".m4a") {
        "AUDIO"
    } else if lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".webp")
        || lower.ends_with(".heic")
        || lower.ends_with(".gif")
    {
        "IMAGE"
    } else {
        "OTHER"
    }
}

fn is_duplicate_error(err: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = err {
        db_err.constraint().is_some_and(|c| c.contains("checksum"))
    } else {
        false
    }
}
