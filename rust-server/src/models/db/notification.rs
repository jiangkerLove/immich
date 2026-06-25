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
