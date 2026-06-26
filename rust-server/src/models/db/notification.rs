use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, Pool, Postgres, QueryBuilder};
use tokio::sync::OnceCell;
use uuid::Uuid;

static NOTIFICATION_TABLE_EXISTS: OnceCell<bool> = OnceCell::const_new();

async fn notification_table_exists(pool: &Pool<Postgres>) -> bool {
    *NOTIFICATION_TABLE_EXISTS
        .get_or_init(|| async {
            sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = 'public' AND table_name = 'notification'
                )
                "#,
            )
            .fetch_one(pool)
            .await
            .unwrap_or(false)
        })
        .await
}

#[derive(Debug, FromRow)]
pub struct NotificationRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub level: String,
    pub notification_type: String,
    pub title: String,
    pub description: Option<String>,
    pub data: Option<Value>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
pub struct NotificationSearchFilter {
    pub id: Option<Uuid>,
    pub level: Option<String>,
    pub notification_type: Option<String>,
    pub unread: Option<bool>,
}

pub async fn search_notifications(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    filter: &NotificationSearchFilter,
) -> Result<Vec<NotificationRow>, sqlx::Error> {
    if !notification_table_exists(pool).await {
        return Ok(vec![]);
    }

    let mut query = QueryBuilder::new(
        r#"
        SELECT
            id,
            "createdAt" AS created_at,
            level,
            type AS notification_type,
            title,
            description,
            data,
            "readAt" AS read_at
        FROM notification
        WHERE "userId" =
        "#,
    );
    query.push_bind(user_id);
    query.push(r#" AND "deletedAt" IS NULL "#);

    if let Some(id) = filter.id {
        query.push(" AND id = ");
        query.push_bind(id);
    }

    if let Some(level) = &filter.level {
        query.push(" AND level = ");
        query.push_bind(level.clone());
    }

    if let Some(notification_type) = &filter.notification_type {
        query.push(" AND type = ");
        query.push_bind(notification_type.clone());
    }

    if filter.unread == Some(true) {
        query.push(r#" AND "readAt" IS NULL "#);
    } else if filter.unread == Some(false) {
        query.push(r#" AND "readAt" IS NOT NULL "#);
    }

    query.push(r#" ORDER BY "createdAt" DESC "#);

    query
        .build_query_as::<NotificationRow>()
        .fetch_all(pool)
        .await
}

pub async fn get_notification(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    id: &Uuid,
) -> Result<Option<NotificationRow>, sqlx::Error> {
    if !notification_table_exists(pool).await {
        return Ok(None);
    }

    sqlx::query_as::<_, NotificationRow>(
        r#"
            SELECT
                id,
                "createdAt" AS created_at,
                level,
                type AS notification_type,
                title,
                description,
                data,
                "readAt" AS read_at
            FROM notification
            WHERE id = $1 AND "userId" = $2 AND "deletedAt" IS NULL
        "#,
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn update_notification(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    id: &Uuid,
    read_at: Option<chrono::DateTime<Utc>>,
) -> Result<Option<NotificationRow>, sqlx::Error> {
    if !notification_table_exists(pool).await {
        return Ok(None);
    }

    sqlx::query_as::<_, NotificationRow>(
        r#"
            UPDATE notification
            SET "readAt" = $1
            WHERE id = $2 AND "userId" = $3 AND "deletedAt" IS NULL
            RETURNING
                id,
                "createdAt" AS created_at,
                level,
                type AS notification_type,
                title,
                description,
                data,
                "readAt" AS read_at
        "#,
    )
    .bind(read_at)
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn update_notifications(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    ids: &[Uuid],
    read_at: Option<chrono::DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    if ids.is_empty() || !notification_table_exists(pool).await {
        return Ok(());
    }

    sqlx::query(
        r#"
            UPDATE notification
            SET "readAt" = $1
            WHERE id = ANY($2) AND "userId" = $3 AND "deletedAt" IS NULL
        "#,
    )
    .bind(read_at)
    .bind(ids)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_notification(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    id: &Uuid,
) -> Result<bool, sqlx::Error> {
    if !notification_table_exists(pool).await {
        return Ok(false);
    }

    let result = sqlx::query(
        r#"
            UPDATE notification
            SET "deletedAt" = NOW()
            WHERE id = $1 AND "userId" = $2 AND "deletedAt" IS NULL
        "#,
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_notifications(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if ids.is_empty() || !notification_table_exists(pool).await {
        return Ok(());
    }

    sqlx::query(
        r#"
            UPDATE notification
            SET "deletedAt" = NOW()
            WHERE id = ANY($1) AND "userId" = $2 AND "deletedAt" IS NULL
        "#,
    )
    .bind(ids)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn create_notification(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    level: &str,
    notification_type: &str,
    title: &str,
    description: Option<&str>,
    data: Option<Value>,
    read_at: Option<DateTime<Utc>>,
) -> Result<NotificationRow, sqlx::Error> {
    if !notification_table_exists(pool).await {
        return Err(sqlx::Error::RowNotFound);
    }

    sqlx::query_as::<_, NotificationRow>(
        r#"
            INSERT INTO notification ("userId", level, type, title, description, data, "readAt")
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING
                id,
                "createdAt" AS created_at,
                level,
                type AS notification_type,
                title,
                description,
                data,
                "readAt" AS read_at
        "#,
    )
    .bind(user_id)
    .bind(level)
    .bind(notification_type)
    .bind(title)
    .bind(description)
    .bind(data)
    .bind(read_at)
    .fetch_one(pool)
    .await
}

pub async fn filter_owned_ids(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if ids.is_empty() || !notification_table_exists(pool).await {
        return Ok(vec![]);
    }

    sqlx::query_scalar(
        r#"
            SELECT id FROM notification
            WHERE id = ANY($1) AND "userId" = $2 AND "deletedAt" IS NULL
        "#,
    )
    .bind(ids)
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn cleanup_old(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    if !notification_table_exists(pool).await {
        return Ok(());
    }

    sqlx::query(
        r#"
        DELETE FROM notification
        WHERE ("deletedAt" IS NOT NULL AND "deletedAt" < NOW() - INTERVAL '3 days')
           OR ("readAt" IS NOT NULL
               AND "readAt" > NOW() - INTERVAL '2 days'
               AND "createdAt" < NOW() - INTERVAL '15 days')
           OR ("readAt" IS NULL AND "createdAt" < NOW() - INTERVAL '30 days')
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}
