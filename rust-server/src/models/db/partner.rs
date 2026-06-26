use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartnerDirection {
    SharedBy,
    SharedWith,
}

impl PartnerDirection {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "shared-by" => Some(Self::SharedBy),
            "shared-with" => Some(Self::SharedWith),
            _ => None,
        }
    }
}

#[derive(Debug, FromRow)]
pub struct PartnerRow {
    pub shared_by_id: Uuid,
    pub shared_with_id: Uuid,
    pub in_timeline: bool,
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub profile_image_path: String,
    pub avatar_color: Option<String>,
    pub profile_changed_at: DateTime<Utc>,
}

const PARTNER_USER_SELECT: &str = r#"
    p."sharedById" as shared_by_id,
    p."sharedWithId" as shared_with_id,
    p."inTimeline" as in_timeline,
    u.id as user_id,
    u.email,
    u.name,
    u."profileImagePath" as profile_image_path,
    u."avatarColor" as avatar_color,
    u."profileChangedAt" as profile_changed_at
"#;

pub async fn get_all(pool: &Pool<Postgres>, user_id: &Uuid) -> Result<Vec<PartnerRow>, sqlx::Error> {
    sqlx::query_as::<_, PartnerRow>(&format!(
        r#"
            SELECT {PARTNER_USER_SELECT}
            FROM partner p
            INNER JOIN "user" shared_by ON shared_by.id = p."sharedById" AND shared_by."deletedAt" IS NULL
            INNER JOIN "user" shared_with ON shared_with.id = p."sharedWithId" AND shared_with."deletedAt" IS NULL
            INNER JOIN "user" u ON u.id = CASE
                WHEN p."sharedById" = $1 THEN p."sharedWithId"
                ELSE p."sharedById"
            END
            WHERE p."sharedById" = $1 OR p."sharedWithId" = $1
        "#
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn get(
    pool: &Pool<Postgres>,
    shared_by_id: &Uuid,
    shared_with_id: &Uuid,
) -> Result<Option<(bool,)>, sqlx::Error> {
    sqlx::query_as::<_, (bool,)>(
        r#"
            SELECT "inTimeline" as in_timeline
            FROM partner
            WHERE "sharedById" = $1 AND "sharedWithId" = $2
        "#,
    )
    .bind(shared_by_id)
    .bind(shared_with_id)
    .fetch_optional(pool)
    .await
}

pub async fn create(
    pool: &Pool<Postgres>,
    shared_by_id: &Uuid,
    shared_with_id: &Uuid,
) -> Result<PartnerRow, sqlx::Error> {
    sqlx::query_as::<_, PartnerRow>(&format!(
        r#"
            WITH inserted AS (
                INSERT INTO partner ("sharedById", "sharedWithId")
                VALUES ($1, $2)
                RETURNING "sharedById", "sharedWithId", "inTimeline"
            )
            SELECT
                i."sharedById" as shared_by_id,
                i."sharedWithId" as shared_with_id,
                i."inTimeline" as in_timeline,
                u.id as user_id,
                u.email,
                u.name,
                u."profileImagePath" as profile_image_path,
                u."avatarColor" as avatar_color,
                u."profileChangedAt" as profile_changed_at
            FROM inserted i
            INNER JOIN "user" u ON u.id = i."sharedWithId"
        "#
    ))
    .bind(shared_by_id)
    .bind(shared_with_id)
    .fetch_one(pool)
    .await
}

pub async fn update_in_timeline(
    pool: &Pool<Postgres>,
    shared_by_id: &Uuid,
    shared_with_id: &Uuid,
    in_timeline: bool,
) -> Result<PartnerRow, sqlx::Error> {
    sqlx::query_as::<_, PartnerRow>(&format!(
        r#"
            WITH updated AS (
                UPDATE partner
                SET "inTimeline" = $3
                WHERE "sharedById" = $1 AND "sharedWithId" = $2
                RETURNING "sharedById", "sharedWithId", "inTimeline"
            )
            SELECT
                u2."sharedById" as shared_by_id,
                u2."sharedWithId" as shared_with_id,
                u2."inTimeline" as in_timeline,
                u.id as user_id,
                u.email,
                u.name,
                u."profileImagePath" as profile_image_path,
                u."avatarColor" as avatar_color,
                u."profileChangedAt" as profile_changed_at
            FROM updated u2
            INNER JOIN "user" u ON u.id = u2."sharedById"
        "#
    ))
    .bind(shared_by_id)
    .bind(shared_with_id)
    .bind(in_timeline)
    .fetch_one(pool)
    .await
}

pub async fn remove(
    pool: &Pool<Postgres>,
    shared_by_id: &Uuid,
    shared_with_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            DELETE FROM partner
            WHERE "sharedById" = $1 AND "sharedWithId" = $2
        "#,
    )
    .bind(shared_by_id)
    .bind(shared_with_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn partner_exists_for_update(
    pool: &Pool<Postgres>,
    shared_by_id: &Uuid,
    shared_with_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let exists: Option<(i32,)> = sqlx::query_as(
        r#"
            SELECT 1 as exists
            FROM partner
            WHERE "sharedById" = $1 AND "sharedWithId" = $2
        "#,
    )
    .bind(shared_by_id)
    .bind(shared_with_id)
    .fetch_optional(pool)
    .await?;
    Ok(exists.is_some())
}

pub async fn get_timeline_partner_ids(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT "sharedById"
            FROM partner
            WHERE "sharedWithId" = $1
              AND "inTimeline" = TRUE
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}
