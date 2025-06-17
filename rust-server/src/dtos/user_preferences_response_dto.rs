use crate::db::user_metadata::UserPreferencePO;
use axum::body::Body;
use axum::http::Response;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPreferenceResponseDto {
    pub folders: FoldersResponse,
    pub memories: MemoriesResponse,
    pub people: PeopleResponse,
    pub shared_links: SharedLinksResponse,
    pub ratings: RatingsResponse,
    pub tags: TagsResponse,
    pub email_notifications: EmailNotificationsResponse,
    pub download: DownloadResponse,
    pub purchase: PurchaseResponse,
}

impl From<UserPreferencePO> for UserPreferenceResponseDto {
    fn from(po: UserPreferencePO) -> Self {
        UserPreferenceResponseDto {
            folders: FoldersResponse {
                enabled: po.folders.enabled,
                sidebar_web: po.folders.sidebar_web,
            },
            memories: MemoriesResponse {
                enabled: po.memories.enabled,
            },
            people: PeopleResponse {
                enabled: po.people.enabled,
                sidebar_web: po.people.sidebar_web,
            },
            shared_links: SharedLinksResponse {
                enabled: po.shared_links.enabled,
                sidebar_web: po.shared_links.sidebar_web,
            },
            ratings: RatingsResponse {
                enabled: po.ratings.enabled
            },
            tags: TagsResponse {
                enabled: po.tags.enabled,
                sidebar_web: po.tags.sidebar_web,
            },
            email_notifications: EmailNotificationsResponse {
                enabled: po.email_notifications.enabled,
                album_invite: po.email_notifications.album_invite,
                album_update: po.email_notifications.album_update,
            },
            download: DownloadResponse {
                archive_size: po.download.archive_size,
                include_embedded_videos: po.download.include_embedded_videos,
            },
            purchase: PurchaseResponse {
                show_support_badge: po.purchase.show_support_badge,
                hide_buy_button_until: po.purchase.hide_buy_button_until,
            },
        }
    }
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldersResponse {
    pub enabled: bool,
    pub sidebar_web: bool,
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoriesResponse {
    pub enabled: bool,
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeopleResponse {
    pub enabled: bool,
    pub sidebar_web: bool,
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RatingsResponse {
    pub enabled: bool,
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedLinksResponse {
    pub enabled: bool,
    pub sidebar_web: bool,
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagsResponse {
    pub enabled: bool,
    pub sidebar_web: bool,
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailNotificationsResponse {
    pub enabled: bool,
    pub album_invite: bool,
    pub album_update: bool,
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResponse {
    pub archive_size: i64,
    pub include_embedded_videos: bool,
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseResponse {
    pub show_support_badge: bool,
    pub hide_buy_button_until: String,
}

impl IntoResponse for UserPreferenceResponseDto {
    fn into_response(self) -> Response<Body> {
        Response::new(Body::from(serde_json::to_string(&self).unwrap()))
    }
}