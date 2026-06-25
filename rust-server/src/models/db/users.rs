use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewUserDb {
    pub email: String,
    pub password: String,
    pub name: String,
    pub is_admin: bool,
    pub storage_label: Option<String>,
}

#[derive(Debug, FromRow)]
pub struct UserDb {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub profile_image_path: String,
    pub should_change_password: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub oauth_id: String,
    pub updated_at: DateTime<Utc>,
    pub storage_label: Option<String>,
    pub name: String,
    pub quota_size_in_bytes: Option<i64>,
    pub quota_usage_in_bytes: i64,
    #[sqlx(try_from = "String")]
    pub status: UserStatus,
    pub profile_changed_at: DateTime<Utc>,
    pub update_id: Uuid,
    pub avatar_color: Option<String>,
    pub pin_code: Option<String>,
    pub email: String,
    pub password: String,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuthUserDb {
    pub id: Uuid,
    pub is_admin: bool,
    pub name: String,
    pub email: String,
    pub quota_usage_in_bytes: i64,
    pub quota_size_in_bytes: Option<i64>,
}

#[derive(Debug, FromRow)]
pub struct UserPinAuthDb {
    pub pin_code: Option<String>,
    pub password: String,
}

const USER_SELECT: &str = r#"
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
    FROM "user"
"#;

impl UserDb {
    pub async fn select_by_oauth_id(
        pool: &Pool<Postgres>,
        oauth_id: &str,
    ) -> Result<Option<UserDb>, sqlx::Error> {
        let query = format!(r#"{USER_SELECT} WHERE "oauthId" = $1 AND "deletedAt" IS NULL"#);
        sqlx::query_as::<_, Self>(&query)
            .bind(oauth_id)
            .fetch_optional(pool)
            .await
    }

    pub async fn select_full_by_email(
        pool: &Pool<Postgres>,
        user_email: &str,
    ) -> Result<Option<UserDb>, sqlx::Error> {
        let query = format!(
            r#"{USER_SELECT} WHERE email = $1 AND "deletedAt" IS NULL"#
        );
        sqlx::query_as::<_, Self>(&query)
            .bind(user_email)
            .fetch_optional(pool)
            .await
    }

    pub async fn select_full_by_id(
        pool: &Pool<Postgres>,
        id: &Uuid,
    ) -> Result<Option<UserDb>, sqlx::Error> {
        let query = format!(r#"{USER_SELECT} WHERE id = $1 AND "deletedAt" IS NULL"#);
        sqlx::query_as::<_, Self>(&query)
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn get_admin(pool: &Pool<Postgres>) -> Result<Option<UserDb>, sqlx::Error> {
        let query = format!(
            r#"{USER_SELECT} WHERE "isAdmin" = true AND "deletedAt" IS NULL LIMIT 1"#
        );
        sqlx::query_as::<_, Self>(&query)
            .fetch_optional(pool)
            .await
    }

    pub async fn get_for_pin_code(
        pool: &Pool<Postgres>,
        id: &Uuid,
    ) -> Result<Option<UserPinAuthDb>, sqlx::Error> {
        sqlx::query_as::<_, UserPinAuthDb>(
            r#"
                SELECT "pinCode" as "pin_code", password
                FROM "user"
                WHERE id = $1 AND "deletedAt" IS NULL
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn insert(pool: &Pool<Postgres>, user: &NewUserDb) -> Result<UserDb, sqlx::Error> {
        sqlx::query_as::<_, UserDb>(
            r#"
                INSERT INTO "user" (email, password, name, "isAdmin", "storageLabel")
                VALUES ($1, $2, $3, $4, $5)
                RETURNING
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
            "#,
        )
        .bind(&user.email)
        .bind(&user.password)
        .bind(&user.name)
        .bind(user.is_admin)
        .bind(&user.storage_label)
        .fetch_one(pool)
        .await
    }
}

impl AuthUserDb {
    pub async fn select_user_by_id(
        pool: &Pool<Postgres>,
        uuid: &Uuid,
    ) -> Result<Option<AuthUserDb>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"
                SELECT
                    id,
                    name,
                    "quotaSizeInBytes" as "quota_size_in_bytes",
                    "quotaUsageInBytes" as "quota_usage_in_bytes",
                    email,
                    "isAdmin" as "is_admin"
                FROM "user"
                WHERE id = $1 AND "deletedAt" IS NULL
            "#,
        )
        .bind(uuid)
        .fetch_optional(pool)
        .await
    }
}

#[derive(Debug, Serialize)]
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
            _ => Err(format!("Invalid status: {s}")),
        }
    }
}

impl TryFrom<String> for UserStatus {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

pub fn map_user_admin(user: UserDb) -> crate::models::response::user::UserAdminResponse {
    crate::models::response::user::UserAdminResponse {
        id: user.id.to_string(),
        email: user.email,
        name: user.name,
        profile_image_path: user.profile_image_path,
        avatar_color: user.avatar_color.unwrap_or_default(),
        profile_changed_at: user.profile_changed_at,
        storage_label: user.storage_label.unwrap_or_default(),
        should_change_password: user.should_change_password,
        is_admin: user.is_admin,
        created_at: user.created_at,
        deleted_at: user.deleted_at,
        updated_at: user.updated_at,
        oauth_id: user.oauth_id,
        quota_size_in_bytes: user.quota_size_in_bytes,
        quota_usage_in_bytes: user.quota_usage_in_bytes,
        status: user.status.as_str().to_string(),
        license: None,
    }
}
