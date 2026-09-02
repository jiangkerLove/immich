use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

use super::cluster_group;
use super::person_schema::PersonSchema;

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

    pub async fn get_by_email(
        pool: &Pool<Postgres>,
        email: &str,
    ) -> Result<Option<UserDb>, sqlx::Error> {
        let query = format!(
            r#"{USER_SELECT} WHERE email = $1 AND "deletedAt" IS NULL"#
        );
        sqlx::query_as::<_, Self>(&query)
            .bind(email)
            .fetch_optional(pool)
            .await
    }

    pub async fn get_for_change_password(
        pool: &Pool<Postgres>,
        id: &Uuid,
    ) -> Result<Option<UserPinAuthDb>, sqlx::Error> {
        Self::get_for_pin_code(pool, id).await
    }

    pub async fn update_me(
        pool: &Pool<Postgres>,
        id: &Uuid,
        email: Option<&str>,
        name: Option<&str>,
        avatar_color: Option<Option<&str>>,
        password: Option<&str>,
    ) -> Result<UserDb, sqlx::Error> {
        let current = Self::select_full_by_id(pool, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        let email = email.unwrap_or(&current.email);
        let name = name.unwrap_or(&current.name);
        let avatar_color = match avatar_color {
            Some(value) => value.map(|s| s.to_string()),
            None => current.avatar_color.clone(),
        };
        let (password, should_change_password) = match password {
            Some(value) => (value.to_string(), false),
            None => (current.password.clone(), current.should_change_password),
        };

        sqlx::query_as::<_, UserDb>(
            r#"
                UPDATE "user"
                SET email = $1,
                    name = $2,
                    "avatarColor" = $3,
                    password = $4,
                    "shouldChangePassword" = $5
                WHERE id = $6 AND "deletedAt" IS NULL
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
        .bind(email)
        .bind(name)
        .bind(avatar_color)
        .bind(password)
        .bind(should_change_password)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn update_password(
        pool: &Pool<Postgres>,
        id: &Uuid,
        password: &str,
    ) -> Result<UserDb, sqlx::Error> {
        Self::update_me(pool, id, None, None, None, Some(password)).await
    }

    pub async fn update_pin_code(
        pool: &Pool<Postgres>,
        id: &Uuid,
        pin_code: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"UPDATE "user" SET "pinCode" = $1 WHERE id = $2"#)
            .bind(pin_code)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn update_profile_image(
        pool: &Pool<Postgres>,
        id: &Uuid,
        profile_image_path: &str,
    ) -> Result<UserDb, sqlx::Error> {
        let query = format!(
            r#"
                UPDATE "user"
                SET "profileImagePath" = $1,
                    "profileChangedAt" = NOW()
                WHERE id = $2 AND "deletedAt" IS NULL
                RETURNING {USER_SELECT}
            "#
        );
        sqlx::query_as::<_, UserDb>(&query)
            .bind(profile_image_path)
            .bind(id)
            .fetch_one(pool)
            .await
    }

    pub async fn clear_profile_image(pool: &Pool<Postgres>, id: &Uuid) -> Result<UserDb, sqlx::Error> {
        Self::update_profile_image(pool, id, "").await
    }

    pub async fn get_profile_image_path(
        pool: &Pool<Postgres>,
        id: &Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT "profileImagePath" FROM "user" WHERE id = $1 AND "deletedAt" IS NULL"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn insert(pool: &Pool<Postgres>, user: &NewUserDb) -> Result<UserDb, sqlx::Error> {
        let schema = PersonSchema::get(pool).await?;
        if schema.is_cluster_groups() {
            let cluster_group_id = cluster_group::create(pool).await?;
            return sqlx::query_as::<_, UserDb>(
                r#"
                INSERT INTO "user" (email, password, name, "isAdmin", "storageLabel", "clusterGroupId")
                VALUES ($1, $2, $3, $4, $5, $6)
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
            .bind(cluster_group_id)
            .fetch_one(pool)
            .await;
        }

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

    pub async fn list_admin(
        pool: &Pool<Postgres>,
        id: Option<&Uuid>,
        with_deleted: bool,
    ) -> Result<Vec<UserDb>, sqlx::Error> {
        let mut query = format!(r#"{USER_SELECT} WHERE 1=1"#);
        if !with_deleted {
            query.push_str(r#" AND "deletedAt" IS NULL"#);
        }
        if id.is_some() {
            query.push_str(" AND id = $1");
        }
        query.push_str(r#" ORDER BY "createdAt" DESC"#);

        let mut builder = sqlx::query_as::<_, UserDb>(&query);
        if let Some(user_id) = id {
            builder = builder.bind(user_id);
        }
        builder.fetch_all(pool).await
    }

    pub async fn select_by_id_admin(
        pool: &Pool<Postgres>,
        id: &Uuid,
        with_deleted: bool,
    ) -> Result<Option<UserDb>, sqlx::Error> {
        let query = if with_deleted {
            format!(r#"{USER_SELECT} WHERE id = $1"#)
        } else {
            format!(r#"{USER_SELECT} WHERE id = $1 AND "deletedAt" IS NULL"#)
        };
        sqlx::query_as::<_, UserDb>(&query)
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn get_by_storage_label(
        pool: &Pool<Postgres>,
        storage_label: &str,
    ) -> Result<Option<UserDb>, sqlx::Error> {
        let query = format!(
            r#"{USER_SELECT} WHERE "storageLabel" = $1 AND "deletedAt" IS NULL"#
        );
        sqlx::query_as::<_, UserDb>(&query)
            .bind(storage_label)
            .fetch_optional(pool)
            .await
    }

    pub async fn admin_create(
        pool: &Pool<Postgres>,
        email: &str,
        password: &str,
        name: &str,
        is_admin: bool,
        storage_label: Option<&str>,
        avatar_color: Option<&str>,
        pin_code: Option<&str>,
        quota_size_in_bytes: Option<i64>,
        should_change_password: bool,
    ) -> Result<UserDb, sqlx::Error> {
        let schema = PersonSchema::get(pool).await?;
        if schema.is_cluster_groups() {
            let cluster_group_id = cluster_group::create(pool).await?;
            return sqlx::query_as::<_, UserDb>(
                r#"
                INSERT INTO "user" (
                    email, password, name, "isAdmin", "storageLabel",
                    "avatarColor", "pinCode", "quotaSizeInBytes", "shouldChangePassword",
                    "clusterGroupId"
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
            .bind(email)
            .bind(password)
            .bind(name)
            .bind(is_admin)
            .bind(storage_label)
            .bind(avatar_color)
            .bind(pin_code)
            .bind(quota_size_in_bytes)
            .bind(should_change_password)
            .bind(cluster_group_id)
            .fetch_one(pool)
            .await;
        }

        sqlx::query_as::<_, UserDb>(
            r#"
                INSERT INTO "user" (
                    email, password, name, "isAdmin", "storageLabel",
                    "avatarColor", "pinCode", "quotaSizeInBytes", "shouldChangePassword"
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
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
        .bind(email)
        .bind(password)
        .bind(name)
        .bind(is_admin)
        .bind(storage_label)
        .bind(avatar_color)
        .bind(pin_code)
        .bind(quota_size_in_bytes)
        .bind(should_change_password)
        .fetch_one(pool)
        .await
    }

    pub async fn admin_update(
        pool: &Pool<Postgres>,
        id: &Uuid,
        email: Option<&str>,
        password: Option<&str>,
        name: Option<&str>,
        avatar_color: Option<Option<&str>>,
        pin_code: Option<Option<&str>>,
        storage_label: Option<Option<&str>>,
        quota_size_in_bytes: Option<Option<i64>>,
        should_change_password: Option<bool>,
        is_admin: Option<bool>,
    ) -> Result<UserDb, sqlx::Error> {
        let current = Self::select_by_id_admin(pool, id, false)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        let email = email.unwrap_or(&current.email);
        let name = name.unwrap_or(&current.name);
        let password = password.unwrap_or(&current.password);
        let avatar_color = match avatar_color {
            Some(value) => value.map(|s| s.to_string()),
            None => current.avatar_color.clone(),
        };
        let pin_code = match pin_code {
            Some(value) => value.map(|s| s.to_string()),
            None => current.pin_code.clone(),
        };
        let storage_label = match storage_label {
            Some(value) => value.map(|s| s.to_string()),
            None => current.storage_label.clone(),
        };
        let quota_size_in_bytes = match quota_size_in_bytes {
            Some(value) => value,
            None => current.quota_size_in_bytes,
        };
        let should_change_password =
            should_change_password.unwrap_or(current.should_change_password);
        let is_admin = is_admin.unwrap_or(current.is_admin);

        sqlx::query_as::<_, UserDb>(
            r#"
                UPDATE "user"
                SET email = $1,
                    password = $2,
                    name = $3,
                    "avatarColor" = $4,
                    "pinCode" = $5,
                    "storageLabel" = $6,
                    "quotaSizeInBytes" = $7,
                    "shouldChangePassword" = $8,
                    "isAdmin" = $9,
                    "updatedAt" = NOW()
                WHERE id = $10 AND "deletedAt" IS NULL
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
        .bind(email)
        .bind(password)
        .bind(name)
        .bind(avatar_color)
        .bind(pin_code)
        .bind(storage_label)
        .bind(quota_size_in_bytes)
        .bind(should_change_password)
        .bind(is_admin)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn count_active(pool: &Pool<Postgres>) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(r#"SELECT COUNT(*)::bigint FROM "user" WHERE "deletedAt" IS NULL"#)
            .fetch_one(pool)
            .await
    }

    pub async fn admin_delete(
        pool: &Pool<Postgres>,
        id: &Uuid,
        force: bool,
    ) -> Result<UserDb, sqlx::Error> {
        let status = if force {
            UserStatus::Removing.as_str()
        } else {
            UserStatus::Deleted.as_str()
        };

        sqlx::query(
            r#"UPDATE album SET "deletedAt" = NOW() WHERE "ownerId" = $1 AND "deletedAt" IS NULL"#,
        )
        .bind(id)
        .execute(pool)
        .await?;

        sqlx::query_as::<_, UserDb>(
            r#"
                UPDATE "user"
                SET status = $1,
                    "deletedAt" = NOW(),
                    "updatedAt" = NOW()
                WHERE id = $2 AND "deletedAt" IS NULL
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
        .bind(status)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn admin_restore(pool: &Pool<Postgres>, id: &Uuid) -> Result<UserDb, sqlx::Error> {
        sqlx::query(
            r#"UPDATE album SET "deletedAt" = NULL WHERE "ownerId" = $1"#,
        )
        .bind(id)
        .execute(pool)
        .await?;

        sqlx::query_as::<_, UserDb>(
            r#"
                UPDATE "user"
                SET status = 'active',
                    "deletedAt" = NULL,
                    "updatedAt" = NOW()
                WHERE id = $1
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
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn sync_usage(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
                UPDATE "user" u
                SET "quotaUsageInBytes" = COALESCE((
                    SELECT SUM(e."fileSizeInByte")
                    FROM asset a
                    LEFT JOIN asset_exif e ON e."assetId" = a.id
                    WHERE a."ownerId" = u.id
                      AND a."libraryId" IS NULL
                ), 0),
                "updatedAt" = NOW()
                WHERE u.id = $1 AND u."deletedAt" IS NULL
            "#,
        )
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list_deleted_before(
        pool: &Pool<Postgres>,
        before: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"
                SELECT id
                FROM "user"
                WHERE "deletedAt" IS NOT NULL
                  AND "deletedAt" < $1
            "#,
        )
        .bind(before)
        .fetch_all(pool)
        .await
    }

    pub async fn hard_delete(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM album WHERE "ownerId" = $1"#)
            .bind(id)
            .execute(pool)
            .await?;
        sqlx::query(r#"DELETE FROM "user" WHERE id = $1"#)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn unlink_all_oauth(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
        sqlx::query(r#"UPDATE "user" SET "oauthId" = ''"#)
            .execute(pool)
            .await?;
        Ok(())
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
    Removing,
    Deleted,
    Inactive,
    Pending,
    Suspended,
}

impl UserStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Removing => "removing",
            Self::Deleted => "deleted",
            Self::Inactive => "inactive",
            Self::Pending => "pending",
            Self::Suspended => "suspended",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "active" => Ok(Self::Active),
            "removing" => Ok(Self::Removing),
            "deleted" => Ok(Self::Deleted),
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

pub fn map_user(user: UserDb) -> crate::models::response::user::UserResponse {
    crate::models::response::user::UserResponse {
        id: user.id.to_string(),
        email: user.email,
        name: user.name,
        profile_image_path: user.profile_image_path,
        avatar_color: user.avatar_color.unwrap_or_default(),
        profile_changed_at: user.profile_changed_at,
    }
}

pub fn map_user_admin(
    user: UserDb,
    license: Option<crate::models::response::user::UserLicenseResponse>,
) -> crate::models::response::user::UserAdminResponse {
    crate::models::response::user::UserAdminResponse {
        id: user.id.to_string(),
        email: user.email,
        name: user.name,
        profile_image_path: user.profile_image_path,
        avatar_color: user.avatar_color.unwrap_or_default(),
        profile_changed_at: user.profile_changed_at,
        storage_label: user.storage_label,
        should_change_password: user.should_change_password,
        is_admin: user.is_admin,
        created_at: user.created_at,
        deleted_at: user.deleted_at,
        updated_at: user.updated_at,
        oauth_id: user.oauth_id,
        quota_size_in_bytes: user.quota_size_in_bytes,
        quota_usage_in_bytes: user.quota_usage_in_bytes,
        status: user.status.as_str().to_string(),
        license,
    }
}

pub fn map_license(
    license: crate::models::db::user_metadata::UserLicensePO,
) -> crate::models::response::user::UserLicenseResponse {
    let activated_at = license
        .activated_at
        .parse::<DateTime<Utc>>()
        .unwrap_or_else(|_| Utc::now());
    crate::models::response::user::UserLicenseResponse {
        license_key: license.license_key,
        activation_key: license.activation_key,
        activated_at,
    }
}

pub async fn map_user_admin_with_license(
    pool: &Pool<Postgres>,
    user: UserDb,
) -> Result<crate::models::response::user::UserAdminResponse, sqlx::Error> {
    use crate::models::db::user_metadata::UserMetadataPO;

    let license = UserMetadataPO::get_license(pool, &user.id)
        .await?
        .map(map_license);
    Ok(map_user_admin(user, license))
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserStatsRow {
    pub user_id: Uuid,
    pub user_name: String,
    pub quota_size_in_bytes: Option<i64>,
    pub photos: i64,
    pub videos: i64,
    pub usage: i64,
    pub usage_photos: i64,
    pub usage_videos: i64,
}

pub async fn get_user_stats(pool: &Pool<Postgres>) -> Result<Vec<UserStatsRow>, sqlx::Error> {
    sqlx::query_as::<_, UserStatsRow>(
        r#"
            SELECT
                u.id as user_id,
                u.name as user_name,
                u."quotaSizeInBytes" as quota_size_in_bytes,
                COUNT(*) FILTER (
                    WHERE a.type = 'IMAGE' AND a.visibility != 'hidden'
                ) as photos,
                COUNT(*) FILTER (
                    WHERE a.type = 'VIDEO' AND a.visibility != 'hidden'
                ) as videos,
                COALESCE(
                    SUM(e."fileSizeInByte") FILTER (WHERE a."libraryId" IS NULL),
                    0
                ) as usage,
                COALESCE(
                    SUM(e."fileSizeInByte") FILTER (
                        WHERE a."libraryId" IS NULL AND a.type = 'IMAGE'
                    ),
                    0
                ) as usage_photos,
                COALESCE(
                    SUM(e."fileSizeInByte") FILTER (
                        WHERE a."libraryId" IS NULL AND a.type = 'VIDEO'
                    ),
                    0
                ) as usage_videos
            FROM "user" u
            LEFT JOIN asset a ON a."ownerId" = u.id AND a."deletedAt" IS NULL
            LEFT JOIN asset_exif e ON e."assetId" = a.id
            WHERE u."deletedAt" IS NULL
            GROUP BY u.id
            ORDER BY u."createdAt" ASC
        "#,
    )
    .fetch_all(pool)
    .await
}
