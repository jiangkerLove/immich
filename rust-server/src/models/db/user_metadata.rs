use crate::models::response::response::ErrorResp;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct UserMetadataPO {
    pub user_id: Uuid,
    pub key: String,
    pub value: sqlx::types::Json<UserPreferencePO>,
}

#[derive(Debug, Serialize)]
pub enum UserMetadataKey {
    Preferences,
    License,
    Onboarding,
}

impl UserMetadataKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserMetadataKey::Preferences => "preferences",
            UserMetadataKey::License => "license",
            UserMetadataKey::Onboarding => "onboarding",
        }
    }
}


#[derive(Serialize, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UserPreferencePO {
    pub folders: FoldersPO,
    pub memories: MemoriesPO,
    pub people: PeoplePO,
    pub shared_links: SharedLinksPO,
    pub ratings: RatingsPO,
    pub tags: TagsPO,
    pub email_notifications: EmailNotificationsPO,
    pub download: DownloadPO,
    pub purchase: PurchasePO,
    pub cast: CastPO,
}

impl Default for UserPreferencePO {
    fn default() -> Self {
        UserPreferencePO {
            folders: Default::default(),
            memories: Default::default(),
            people: Default::default(),
            ratings: Default::default(),
            shared_links: Default::default(),
            tags: Default::default(),
            email_notifications: Default::default(),
            download: Default::default(),
            purchase: Default::default(),
            cast: Default::default(),
        }
    }
}


#[derive(Serialize, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FoldersPO {
    pub enabled: bool,
    pub sidebar_web: bool,
}

impl Default for FoldersPO {
    fn default() -> Self {
        FoldersPO {
            enabled: false,
            sidebar_web: false,
        }
    }
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MemoriesPO {
    pub enabled: bool,
}
impl Default for MemoriesPO {
    fn default() -> Self {
        MemoriesPO {
            enabled: true,
        }
    }
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PeoplePO {
    pub enabled: bool,
    pub sidebar_web: bool,
}

impl Default for PeoplePO {
    fn default() -> Self {
        PeoplePO {
            enabled: true,
            sidebar_web: false,
        }
    }
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RatingsPO {
    pub enabled: bool,
}
impl Default for RatingsPO {
    fn default() -> Self {
        RatingsPO {
            enabled: false,
        }
    }
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SharedLinksPO {
    pub enabled: bool,
    pub sidebar_web: bool,
}

impl Default for SharedLinksPO {
    fn default() -> Self {
        SharedLinksPO {
            enabled: true,
            sidebar_web: false,
        }
    }
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TagsPO {
    pub enabled: bool,
    pub sidebar_web: bool,
}

impl Default for TagsPO {
    fn default() -> Self {
        TagsPO {
            enabled: false,
            sidebar_web: false,
        }
    }
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EmailNotificationsPO {
    pub enabled: bool,
    pub album_invite: bool,
    pub album_update: bool,
}

impl Default for EmailNotificationsPO {
    fn default() -> Self {
        EmailNotificationsPO {
            enabled: true,
            album_invite: true,
            album_update: true,
        }
    }
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DownloadPO {
    pub archive_size: i64,
    pub include_embedded_videos: bool,
}

impl Default for DownloadPO {
    fn default() -> Self {
        DownloadPO {
            archive_size: 4i64 * 1024 * 1024 * 1024,
            include_embedded_videos: false,
        }
    }
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PurchasePO {
    pub hide_buy_button_until: String,
    pub show_support_badge: bool,
}

impl Default for PurchasePO {
    fn default() -> Self {
        let naive = NaiveDate::from_ymd_opt(2022, 2, 11)
            .unwrap().and_hms_opt(16, 0, 0).unwrap();

        // 2. 转换为 UTC 时间
        let utc_datetime: DateTime<Utc> = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
        PurchasePO {
            hide_buy_button_until: utc_datetime.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            show_support_badge: true,
        }
    }
}

#[derive(Serialize, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CastPO {
    pub g_cast_enabled: bool,
}

impl Default for CastPO {
    fn default() -> Self {
        CastPO { g_cast_enabled: false }
    }
}


#[derive(Clone, Serialize, Debug, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct OnboardingPO {
    pub is_onboarded: bool,
}

impl UserMetadataPO {
    pub async fn get_meta_data_by_uid(
        pool: &Pool<Postgres>,
        id: &Uuid,
    ) -> Result<Vec<UserMetadataPO>, ErrorResp> {
        let maybe_user = sqlx::query_as::<_, Self>(
            r#"
                SELECT
                    key,
                    "userId" as "user_id",
                    value
                FROM user_metadata
                WHERE "userId" = $1
            "#,
        )
        .bind(id)
        .fetch_all(pool)
        .await?;
        Ok(maybe_user)
    }

    pub async fn is_onboarded(pool: &Pool<Postgres>, user_id: &Uuid) -> Result<bool, sqlx::Error> {
        Ok(Self::get_onboarding(pool, user_id).await?.is_onboarded)
    }

    pub async fn get_onboarding(
        pool: &Pool<Postgres>,
        user_id: &Uuid,
    ) -> Result<OnboardingPO, sqlx::Error> {
        let row: Option<(sqlx::types::Json<OnboardingPO>,)> = sqlx::query_as(
            r#"
                SELECT value
                FROM user_metadata
                WHERE "userId" = $1 AND key = 'onboarding'
            "#,
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|(value,)| value.0).unwrap_or_default())
    }

    pub async fn upsert_onboarding(
        pool: &Pool<Postgres>,
        user_id: &Uuid,
        onboarding: &OnboardingPO,
    ) -> Result<(), sqlx::Error> {
        let value = serde_json::to_value(onboarding).unwrap_or_default();
        sqlx::query(
            r#"
            INSERT INTO user_metadata ("userId", key, value)
            VALUES ($1, 'onboarding', $2)
            ON CONFLICT ("userId", key) DO UPDATE SET value = EXCLUDED.value
            "#,
        )
        .bind(user_id)
        .bind(value)
        .execute(pool)
        .await?;
        Ok(())
    }
}