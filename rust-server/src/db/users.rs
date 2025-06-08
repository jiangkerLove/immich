use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Pool, Postgres};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewUserPO {
    pub email: String,
    pub password: String,
    pub is_admin: bool,
}


#[derive(Debug, FromRow)]
pub struct UserPO {
    pub id: Uuid,
    pub created_at: OffsetDateTime,
    pub profile_image_path: String,
    pub should_change_password: bool,
    pub deleted_at: Option<OffsetDateTime>,
    pub oauth_id: String,
    pub updated_at: OffsetDateTime,
    pub storage_label: Option<String>,
    pub name: String,
    pub quota_size_in_bytes: Option<i64>,
    pub quota_usage_in_bytes: i64,
    #[sqlx(try_from = "String")]
    pub status: UserStatus,
    pub profile_changed_at: OffsetDateTime,
    pub update_id: Uuid,
    pub avatar_color: Option<String>,
    pub pin_code: Option<String>,
    pub email: String,
    pub password: String,
    pub is_admin: bool,
}

impl UserPO {
    pub async fn select_full_by_email(pool: &Pool<Postgres>, user_email: &String) -> Result<Option<UserPO>, sqlx::Error> {
        let maybe_user = sqlx::query_as::<_, Self>(
            r#"
                    SELECT
                        id,
                        "createdAt" as "created_at",
                        "profileImagePath" as "profile_image_path",
                        "shouldChangePassword" as "should_change_password",
                        "deletedAt" as "deleted_at",
                        "oauthId" as "oauth_id",
                        "updatedAt" as "updated_at",
                        "storageLabel" as "storage_label",
                        name,
                        "quotaSizeInBytes" as "quota_size_in_bytes",
                        "quotaUsageInBytes" as "quota_usage_in_bytes",
                        status,
                        "profileChangedAt" as "profile_changed_at",
                        "updateId" as "update_id",
                        "avatarColor" as "avatar_color",
                        "pinCode" as "pin_code",
                        email,
                        password,
                        "isAdmin" as "is_admin"
                    FROM users
                    WHERE email = $1
                "#,
        ).bind(user_email).fetch_optional(pool).await?;
        Ok(maybe_user)
    }
}

#[derive(Debug)]
pub enum UserStatus {
    Active,
    Inactive,
    Pending,
    Suspended,
}

impl UserStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Pending => "pending",
            Self::Suspended => "suspended",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "pending" => Ok(Self::Pending),
            "suspended" => Ok(Self::Suspended),
            _ => Err(format!("Invalid status: {}", s)),
        }
    }
}

impl TryFrom<String> for UserStatus {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}