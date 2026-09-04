use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::auth_permission::Permission;
use crate::models::db::video_stream;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::access::require_asset_access;
use crate::service::transcoding::HlsEngine;
use crate::utils::storage::StoragePaths;

const HLS_PLAYLIST_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";

#[derive(Clone)]
pub struct HlsService {
    pool: PgPool,
    storage: StoragePaths,
    engine: Arc<HlsEngine>,
}

impl HlsService {
    pub fn new(pool: PgPool, storage: StoragePaths, engine: Arc<HlsEngine>) -> Self {
        Self {
            pool,
            storage,
            engine,
        }
    }

    pub async fn get_main_playlist(
        &self,
        auth: &AuthDto,
        asset_id: Uuid,
    ) -> Result<String, ErrorResp> {
        require_asset_access(&self.pool, auth, &asset_id, Permission::AssetView).await?;
        self.ensure_realtime_enabled().await?;

        let Some(asset) = video_stream::get_for_main_playlist(&self.pool, &asset_id).await? else {
            return Err(ErrorResp::NotFound(
                "Asset metadata is not yet ready for streaming".to_string(),
            ));
        };

        let session_id = self.engine.request_session(asset_id, auth.user.id).await?;
        self.engine.generate_main_playlist(session_id, &asset).await
    }

    pub async fn get_media_playlist(
        &self,
        auth: &AuthDto,
        asset_id: Uuid,
        session_id: Uuid,
        variant_index: u32,
        position: Option<f64>,
    ) -> Result<String, ErrorResp> {
        require_asset_access(&self.pool, auth, &asset_id, Permission::AssetView).await?;

        let Some(asset) =
            video_stream::get_for_media_playlist(&self.pool, &asset_id, &session_id).await?
        else {
            return Err(ErrorResp::NotFound(
                "Asset not found or metadata not yet ready for streaming".to_string(),
            ));
        };

        let hinted_segment = position.map(|value| HlsEngine::position_to_segment(&asset, value));
        self.engine
            .prewarm_variant(asset_id, session_id, variant_index, hinted_segment)
            .await;

        Ok(HlsEngine::generate_media_playlist(&asset))
    }

    pub async fn get_segment_path(
        &self,
        auth: &AuthDto,
        asset_id: Uuid,
        session_id: Uuid,
        variant_index: u32,
        filename: &str,
        init_segment: Option<i32>,
    ) -> Result<std::path::PathBuf, ErrorResp> {
        require_asset_access(&self.pool, auth, &asset_id, Permission::AssetView).await?;

        let Some(session) = video_stream::get_session(&self.pool, &session_id).await? else {
            return Err(ErrorResp::NotFound("Session not found".to_string()));
        };

        let mut api_session = self
            .engine
            .track_api_session(session_id, Some(variant_index))
            .await;
        let segment_index =
            self.engine
                .segment_index_from_filename(&mut api_session, filename, init_segment);

        self.engine
            .heartbeat(session_id, Some(segment_index), Some(variant_index))
            .await;

        let path = self
            .storage
            .hls_variant_folder(&auth.user.id, &session_id, variant_index)
            .join(filename);

        if tokio::fs::metadata(&path).await.is_ok() {
            return Ok(path);
        }

        self.engine
            .wait_for_segment(session_id, variant_index, segment_index, session.asset_id)
            .await?;

        if tokio::fs::metadata(&path).await.is_ok() {
            Ok(path)
        } else {
            Err(ErrorResp::NotFound(format!(
                "Segment {} not found",
                path.display()
            )))
        }
    }

    pub async fn end_session(
        &self,
        auth: &AuthDto,
        asset_id: Uuid,
        session_id: Uuid,
    ) -> Result<(), ErrorResp> {
        require_asset_access(&self.pool, auth, &asset_id, Permission::AssetView).await?;
        self.engine.end_session(session_id).await;
        Ok(())
    }

    pub fn playlist_content_type() -> &'static str {
        HLS_PLAYLIST_CONTENT_TYPE
    }

    pub fn segment_content_type() -> &'static str {
        "video/mp4"
    }

    async fn ensure_realtime_enabled(&self) -> Result<(), ErrorResp> {
        if is_realtime_transcoding_enabled(&self.pool).await? {
            Ok(())
        } else {
            Err(ErrorResp::BadRequest(
                "Real-time transcoding is not enabled".to_string(),
            ))
        }
    }
}

pub async fn is_realtime_transcoding_enabled(pool: &PgPool) -> Result<bool, ErrorResp> {
    let config = crate::models::db::system_metadata::get_json(pool, "system-config").await?;
    Ok(config
        .and_then(|value| {
            value
                .get("ffmpeg")
                .and_then(|ffmpeg| ffmpeg.get("realtime"))
                .and_then(|realtime| realtime.get("enabled"))
                .and_then(|enabled| enabled.as_bool())
        })
        .unwrap_or(false))
}

pub async fn is_maintenance_mode(pool: &PgPool) -> Result<bool, ErrorResp> {
    let value = crate::models::db::system_metadata::get_json(pool, "maintenance-mode").await?;
    Ok(value
        .and_then(|json| json.get("isMaintenanceMode").and_then(|v| v.as_bool()))
        .unwrap_or(false))
}
