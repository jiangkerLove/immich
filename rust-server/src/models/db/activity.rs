use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct ActivityRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub album_id: Uuid,
    pub user_id: Uuid,
    pub asset_id: Option<Uuid>,
    pub comment: Option<String>,
    pub is_liked: bool,
    pub user_email: String,
    pub user_name: String,
    pub user_profile_image_path: String,
    pub user_avatar_color: Option<String>,
    pub user_profile_changed_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub struct ActivityStatisticsRow {
    pub comments: i64,
    pub likes: i64,
}

const ACTIVITY_SELECT: &str = r#"
    SELECT
        a.id,
        a."createdAt" as created_at,
        a."albumId" as album_id,
        a."userId" as user_id,
        a."assetId" as asset_id,
        a.comment,
        a."isLiked" as is_liked,
        u.email as user_email,
        u.name as user_name,
        u."profileImagePath" as user_profile_image_path,
        u."avatarColor" as user_avatar_color,
        u."profileChangedAt" as user_profile_changed_at
    FROM activity a
    INNER JOIN "user" u ON u.id = a."userId" AND u."deletedAt" IS NULL
    LEFT JOIN asset ast ON ast.id = a."assetId"
"#;

pub async fn search_by_album(
    pool: &Pool<Postgres>,
    album_id: &Uuid,
) -> Result<Vec<ActivityRow>, sqlx::Error> {
    sqlx::query_as::<_, ActivityRow>(&format!(
        r#"
            {ACTIVITY_SELECT}
            WHERE a."albumId" = $1
              AND (ast."deletedAt" IS NULL OR ast.id IS NULL)
            ORDER BY a."createdAt" ASC
        "#
    ))
    .bind(album_id)
    .fetch_all(pool)
    .await
}

pub async fn find_like(
    pool: &Pool<Postgres>,
    album_id: &Uuid,
    user_id: &Uuid,
    asset_id: Option<&Uuid>,
) -> Result<Option<ActivityRow>, sqlx::Error> {
    if let Some(asset_id) = asset_id {
        sqlx::query_as::<_, ActivityRow>(&format!(
            r#"
                {ACTIVITY_SELECT}
                WHERE a."albumId" = $1
                  AND a."userId" = $2
                  AND a."assetId" = $3
                  AND a."isLiked" = TRUE
                LIMIT 1
            "#
        ))
        .bind(album_id)
        .bind(user_id)
        .bind(asset_id)
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query_as::<_, ActivityRow>(&format!(
            r#"
                {ACTIVITY_SELECT}
                WHERE a."albumId" = $1
                  AND a."userId" = $2
                  AND a."assetId" IS NULL
                  AND a."isLiked" = TRUE
                LIMIT 1
            "#
        ))
        .bind(album_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }
}

pub async fn create(
    pool: &Pool<Postgres>,
    album_id: &Uuid,
    user_id: &Uuid,
    asset_id: Option<&Uuid>,
    is_liked: bool,
    comment: Option<&str>,
) -> Result<ActivityRow, sqlx::Error> {
    sqlx::query_as::<_, ActivityRow>(
        r#"
            WITH inserted AS (
                INSERT INTO activity ("albumId", "userId", "assetId", "isLiked", comment)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id, "createdAt", "albumId", "userId", "assetId", comment, "isLiked"
            )
            SELECT
                i.id,
                i."createdAt" as created_at,
                i."albumId" as album_id,
                i."userId" as user_id,
                i."assetId" as asset_id,
                i.comment,
                i."isLiked" as is_liked,
                u.email as user_email,
                u.name as user_name,
                u."profileImagePath" as user_profile_image_path,
                u."avatarColor" as user_avatar_color,
                u."profileChangedAt" as user_profile_changed_at
            FROM inserted i
            INNER JOIN "user" u ON u.id = i."userId"
        "#,
    )
    .bind(album_id)
    .bind(user_id)
    .bind(asset_id)
    .bind(is_liked)
    .bind(comment)
    .fetch_one(pool)
    .await
}

pub async fn delete(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM activity WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_statistics(
    pool: &Pool<Postgres>,
    album_id: &Uuid,
    asset_id: Option<&Uuid>,
) -> Result<ActivityStatisticsRow, sqlx::Error> {
    if let Some(asset_id) = asset_id {
        sqlx::query_as::<_, ActivityStatisticsRow>(
            r#"
                SELECT
                    COUNT(*) FILTER (WHERE NOT a."isLiked")::bigint as comments,
                    COUNT(*) FILTER (WHERE a."isLiked")::bigint as likes
                FROM activity a
                INNER JOIN "user" u ON u.id = a."userId" AND u."deletedAt" IS NULL
                LEFT JOIN asset ast ON ast.id = a."assetId"
                WHERE a."albumId" = $1
                  AND a."assetId" = $2
                  AND (
                    (ast."deletedAt" IS NULL AND ast.visibility != 'locked')
                    OR ast.id IS NULL
                  )
            "#,
        )
        .bind(album_id)
        .bind(asset_id)
        .fetch_one(pool)
        .await
    } else {
        sqlx::query_as::<_, ActivityStatisticsRow>(
            r#"
                SELECT
                    COUNT(*) FILTER (WHERE NOT a."isLiked")::bigint as comments,
                    COUNT(*) FILTER (WHERE a."isLiked")::bigint as likes
                FROM activity a
                INNER JOIN "user" u ON u.id = a."userId" AND u."deletedAt" IS NULL
                LEFT JOIN asset ast ON ast.id = a."assetId"
                WHERE a."albumId" = $1
                  AND (
                    (ast."deletedAt" IS NULL AND ast.visibility != 'locked')
                    OR ast.id IS NULL
                  )
            "#,
        )
        .bind(album_id)
        .fetch_one(pool)
        .await
    }
}

pub async fn can_delete(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    activity_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let owner: Option<(i32,)> = sqlx::query_as(
        r#"SELECT 1 as ok FROM activity WHERE id = $1 AND "userId" = $2"#,
    )
    .bind(activity_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    if owner.is_some() {
        return Ok(true);
    }

    let album_owner: Option<(i32,)> = sqlx::query_as(
        r#"
            SELECT 1 as ok
            FROM activity a
            INNER JOIN album al ON al.id = a."albumId" AND al."deletedAt" IS NULL
            INNER JOIN album_user au ON au."albumId" = al.id
            WHERE a.id = $1
              AND au."userId" = $2
              AND au.role = 'owner'
        "#,
    )
    .bind(activity_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(album_owner.is_some())
}

pub async fn album_allows_activity(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    album_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let allowed: Option<(i32,)> = sqlx::query_as(
        r#"
            SELECT 1 as ok
            FROM album a
            INNER JOIN album_user au ON au."albumId" = a.id
            WHERE a.id = $1
              AND au."userId" = $2
              AND a."deletedAt" IS NULL
              AND a."isActivityEnabled" = TRUE
        "#,
    )
    .bind(album_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(allowed.is_some())
}

pub async fn asset_in_album(
    pool: &Pool<Postgres>,
    album_id: &Uuid,
    asset_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let exists: Option<(i32,)> = sqlx::query_as(
        r#"
            SELECT 1 as ok
            FROM album_asset
            WHERE "albumId" = $1 AND "assetId" = $2
        "#,
    )
    .bind(album_id)
    .bind(asset_id)
    .fetch_optional(pool)
    .await?;
    Ok(exists.is_some())
}
