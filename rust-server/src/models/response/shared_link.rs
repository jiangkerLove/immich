use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::models::db::assets::AssetDetailRow;
use crate::models::db::shared_links::SharedLinkRow;
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::{map_asset, AssetResponse};
use crate::service::album::AlbumResponse;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedLinkAlbumResponse {
    #[serde(flatten)]
    pub album: AlbumResponse,
    pub shared: bool,
    pub has_shared_link: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedLinkResponse {
    pub id: Uuid,
    pub description: Option<String>,
    pub password: Option<String>,
    pub user_id: Uuid,
    pub key: String,
    #[serde(rename = "type")]
    pub link_type: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub assets: Vec<AssetResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<SharedLinkAlbumResponse>,
    pub allow_upload: bool,
    pub allow_download: bool,
    pub show_metadata: bool,
    pub slug: Option<String>,
}

pub fn encode_key(key: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key)
}

pub fn map_shared_link(
    row: &SharedLinkRow,
    assets: &[AssetDetailRow],
    album: Option<SharedLinkAlbumResponse>,
    auth: &AuthDto,
    strip_asset_metadata: bool,
) -> SharedLinkResponse {
    SharedLinkResponse {
        id: row.id,
        description: row.description.clone(),
        password: row.password.clone(),
        user_id: row.user_id,
        key: encode_key(&row.key),
        link_type: row.link_type.clone(),
        created_at: row.created_at,
        expires_at: row.expires_at,
        assets: assets
            .iter()
            .map(|asset| map_asset(asset, None, auth, strip_asset_metadata))
            .collect(),
        album,
        allow_upload: row.allow_upload,
        allow_download: row.allow_download,
        show_metadata: row.show_exif,
        slug: row.slug.clone(),
    }
}
