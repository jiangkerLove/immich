use serde::Serialize;
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

use crate::models::db::assets::{self, is_android_motion_path};
use crate::models::db::auth_permission::Permission;
use crate::models::db::download;
use crate::models::db::user_metadata::{DownloadPO, UserMetadataPO};
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::access::{require_album_access, require_assets_access};
use crate::utils::permission::require_permission;
use crate::utils::preferences::resolve_preferences;
use crate::utils::zip_archive::{archive_entry_name, zip_response, ZipEntry};

const DEFAULT_ARCHIVE_SIZE: i64 = 4 * 1024 * 1024 * 1024;

#[derive(Clone)]
pub struct DownloadService {
    pool: PgPool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfoReq {
    pub asset_ids: Option<Vec<Uuid>>,
    pub album_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub archive_size: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadArchiveReq {
    pub asset_ids: Vec<Uuid>,
    pub edited: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadArchiveInfo {
    pub size: i64,
    pub asset_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResponse {
    pub total_size: i64,
    pub archives: Vec<DownloadArchiveInfo>,
}

impl DownloadService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_download_info(
        &self,
        auth: &AuthDto,
        dto: &DownloadInfoReq,
    ) -> Result<DownloadResponse, ErrorResp> {
        let assets = if let Some(asset_ids) = &dto.asset_ids {
            require_assets_access(&self.pool, auth, asset_ids, Permission::AssetDownload).await?;
            download::download_asset_ids(&self.pool, asset_ids).await?
        } else if let Some(album_id) = &dto.album_id {
            require_album_access(&self.pool, auth, album_id, Permission::AlbumDownload).await?;
            download::download_album_id(&self.pool, album_id).await?
        } else if let Some(user_id) = &dto.user_id {
            require_timeline_download(auth, user_id)?;
            download::download_user_id(&self.pool, user_id).await?
        } else {
            return Err(ErrorResp::BadRequest(
                "assetIds, albumId, or userId is required".to_string(),
            ));
        };

        let target_size = dto.archive_size.unwrap_or(DEFAULT_ARCHIVE_SIZE);
        let preferences = self.load_download_preferences(&auth.user.id).await?;
        let mut motion_ids = HashSet::new();
        let mut archives = Vec::new();
        let mut archive = DownloadArchiveInfo {
            size: 0,
            asset_ids: vec![],
        };

        for asset in assets {
            if let Some(motion_id) = asset.live_photo_video_id {
                motion_ids.insert(motion_id);
            }
            push_asset(&mut archives, &mut archive, asset.id, asset.size.unwrap_or(0), target_size);
        }

        if !motion_ids.is_empty() {
            let motion_ids: Vec<Uuid> = motion_ids.into_iter().collect();
            let motion_assets = download::download_motion_asset_ids(&self.pool, &motion_ids).await?;
            for motion in motion_assets {
                if is_android_motion_path(&motion.original_path)
                    && !preferences.include_embedded_videos
                {
                    continue;
                }
                push_asset(
                    &mut archives,
                    &mut archive,
                    motion.id,
                    motion.size.unwrap_or(0),
                    target_size,
                );
            }
        }

        if !archive.asset_ids.is_empty() {
            archives.push(archive);
        }

        let total_size = archives.iter().map(|item| item.size).sum();
        Ok(DownloadResponse {
            total_size,
            archives,
        })
    }

    pub async fn download_archive(
        &self,
        auth: &AuthDto,
        dto: &DownloadArchiveReq,
    ) -> Result<axum::response::Response, ErrorResp> {
        require_assets_access(&self.pool, auth, &dto.asset_ids, Permission::AssetDownload).await?;

        let edited = dto.edited.unwrap_or(false) || auth.shared_link.is_some();
        let assets = assets::get_for_originals(&self.pool, &dto.asset_ids, edited).await?;
        let asset_map: std::collections::HashMap<Uuid, _> =
            assets.into_iter().map(|asset| (asset.id, asset)).collect();

        let mut entries = Vec::new();
        for asset_id in &dto.asset_ids {
            let Some(asset) = asset_map.get(asset_id) else {
                continue;
            };

            let path = if edited {
                asset
                    .edited_path
                    .clone()
                    .unwrap_or_else(|| asset.original_path.clone())
            } else {
                asset.original_path.clone()
            };

            entries.push(ZipEntry {
                path: path.clone(),
                name: archive_entry_name(&asset.original_file_name, &path),
            });
        }

        zip_response(entries).await
    }

    async fn load_download_preferences(&self, user_id: &Uuid) -> Result<DownloadPO, ErrorResp> {
        let stored = UserMetadataPO::get_preferences_json(&self.pool, user_id).await?;
        let resolved = resolve_preferences(stored);
        Ok(resolved
            .get("download")
            .cloned()
            .and_then(|value| serde_json::from_value::<DownloadPO>(value).ok())
            .unwrap_or_default())
    }
}

fn require_timeline_download(auth: &AuthDto, user_id: &Uuid) -> Result<(), ErrorResp> {
    require_permission(auth, Permission::TimelineDownload)?;
    if auth.user.id != *user_id {
        return Err(ErrorResp::BadRequest(
            "Not found or no timeline.download access".to_string(),
        ));
    }
    Ok(())
}

fn push_asset(
    archives: &mut Vec<DownloadArchiveInfo>,
    archive: &mut DownloadArchiveInfo,
    id: Uuid,
    size: i64,
    target_size: i64,
) {
    archive.asset_ids.push(id);
    archive.size += size;
    if archive.size > target_size {
        archives.push(DownloadArchiveInfo {
            size: archive.size,
            asset_ids: std::mem::take(&mut archive.asset_ids),
        });
        archive.size = 0;
    }
}
