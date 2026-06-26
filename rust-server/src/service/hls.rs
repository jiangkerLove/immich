use sqlx::PgPool;

use crate::models::db::auth_permission::Permission;
use crate::models::db::system_metadata::get_json;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::access::require_asset_access;
use uuid::Uuid;

const HLS_PLAYLIST_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";

#[derive(Clone)]
pub struct HlsService {
    pool: PgPool,
}

impl HlsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_main_playlist(&self, auth: &AuthDto, asset_id: Uuid) -> Result<String, ErrorResp> {
        require_asset_access(&self.pool, auth, &asset_id, Permission::AssetView).await?;
        self.ensure_realtime_enabled().await?;
        Err(ErrorResp::NotImplemented(
            "HLS streaming requires a transcoding worker (not yet available in rust-server)".to_string(),
        ))
    }

    pub async fn get_media_playlist(
        &self,
        auth: &AuthDto,
        asset_id: Uuid,
        _session_id: Uuid,
        _variant_index: u32,
        _position: Option<f64>,
    ) -> Result<String, ErrorResp> {
        require_asset_access(&self.pool, auth, &asset_id, Permission::AssetView).await?;
        self.ensure_realtime_enabled().await?;
        Err(ErrorResp::NotImplemented(
            "HLS streaming requires a transcoding worker (not yet available in rust-server)".to_string(),
        ))
    }

    pub async fn get_segment(
        &self,
        auth: &AuthDto,
        asset_id: Uuid,
        _session_id: Uuid,
        _variant_index: u32,
        _filename: &str,
        _init_segment: Option<u32>,
    ) -> Result<(), ErrorResp> {
        require_asset_access(&self.pool, auth, &asset_id, Permission::AssetView).await?;
        self.ensure_realtime_enabled().await?;
        Err(ErrorResp::NotImplemented(
            "HLS streaming requires a transcoding worker (not yet available in rust-server)".to_string(),
        ))
    }

    pub async fn end_session(
        &self,
        auth: &AuthDto,
        asset_id: Uuid,
        _session_id: Uuid,
    ) -> Result<(), ErrorResp> {
        require_asset_access(&self.pool, auth, &asset_id, Permission::AssetView).await?;
        Ok(())
    }

    pub fn playlist_content_type() -> &'static str {
        HLS_PLAYLIST_CONTENT_TYPE
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
    let config = get_json(pool, "system-config").await?;
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
    let value = get_json(pool, "maintenance-mode").await?;
    Ok(value
        .and_then(|json| json.get("isMaintenanceMode").and_then(|v| v.as_bool()))
        .unwrap_or(false))
}
