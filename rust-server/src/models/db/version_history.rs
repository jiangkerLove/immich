use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::constants::SERVER_VERSION;

static VERSION_HISTORY_TABLE_EXISTS: OnceCell<bool> = OnceCell::const_new();

async fn version_history_table_exists(pool: &Pool<Postgres>) -> bool {
    *VERSION_HISTORY_TABLE_EXISTS
        .get_or_init(|| async {
            sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = 'public' AND table_name = 'version_history'
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
pub struct VersionHistoryRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub version: String,
}

pub async fn get_all(pool: &Pool<Postgres>) -> Result<Vec<VersionHistoryRow>, sqlx::Error> {
    if !version_history_table_exists(pool).await {
        return Ok(vec![]);
    }

    ensure_current_version(pool).await?;

    sqlx::query_as::<_, VersionHistoryRow>(
        r#"
        SELECT id, "createdAt" AS created_at, version
        FROM version_history
        ORDER BY "createdAt" DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

async fn ensure_current_version(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    let latest: Option<String> = sqlx::query_scalar(
        r#"
        SELECT version
        FROM version_history
        ORDER BY "createdAt" DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    if latest.as_deref() == Some(SERVER_VERSION) {
        return Ok(());
    }

    sqlx::query(
        r#"
        INSERT INTO version_history (version)
        VALUES ($1)
        "#,
    )
    .bind(SERVER_VERSION)
    .execute(pool)
    .await?;

    Ok(())
}
