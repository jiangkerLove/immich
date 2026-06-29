use sqlx::{Pool, Postgres};

static SMART_SEARCH_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub async fn smart_search_available(pool: &Pool<Postgres>) -> bool {
    if let Some(value) = SMART_SEARCH_AVAILABLE.get() {
        return *value;
    }

    let available = table_exists(pool, "smart_search").await.unwrap_or(false);
    let _ = SMART_SEARCH_AVAILABLE.set(available);
    if !available {
        eprintln!(
            "smart_search unavailable: pgvector tables missing (install vector extension or use init.sql DO block)"
        );
    }
    available
}

pub async fn face_search_available(pool: &Pool<Postgres>) -> bool {
    table_exists(pool, "face_search").await.unwrap_or(false)
}

async fn table_exists(pool: &Pool<Postgres>, table: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = 'public'
              AND table_name = $1
        )
        "#,
    )
    .bind(table)
    .fetch_one(pool)
    .await
}
