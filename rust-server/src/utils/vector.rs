use sqlx::{Pool, Postgres};

use crate::models::dto::env::DbVectorExtension;

static SMART_SEARCH_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub async fn smart_search_available(pool: &Pool<Postgres>) -> bool {
    if let Some(value) = SMART_SEARCH_AVAILABLE.get() {
        return *value;
    }

    let available = table_exists(pool, "smart_search").await.unwrap_or(false);
    let _ = SMART_SEARCH_AVAILABLE.set(available);
    if !available {
        eprintln!(
            "smart_search unavailable: pgvector tables missing (install vector extension; baseline creates smart_search when available)"
        );
    }
    available
}

pub async fn face_search_available(pool: &Pool<Postgres>) -> bool {
    table_exists(pool, "face_search").await.unwrap_or(false)
}

pub async fn get_dimension_size(pool: &Pool<Postgres>, table: &str, column: &str) -> i32 {
    let dim_size: Option<i32> = sqlx::query_scalar(
        r#"
        SELECT atttypmod AS dimsize
        FROM pg_attribute f
        JOIN pg_class c ON c.oid = f.attrelid
        WHERE c.relkind = 'r'::char
          AND f.attnum > 0
          AND c.relname = $1
          AND f.attname = $2
        "#,
    )
    .bind(table)
    .bind(column)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match dim_size {
        Some(size) if (1..=65536).contains(&size) => size,
        _ => {
            eprintln!(
                "Could not retrieve dimension size of column '{column}' in table '{table}', assuming 512"
            );
            512
        }
    }
}

pub async fn resolve_vector_extension(
    pool: &Pool<Postgres>,
    configured: Option<DbVectorExtension>,
) -> Result<DbVectorExtension, String> {
    if let Some(ext) = configured {
        return Ok(ext);
    }

    let available: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT name
        FROM pg_available_extensions
        WHERE name IN ('vector', 'vchordrq', 'vectors')
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|err| err.to_string())?;

    let names: std::collections::HashSet<_> = available.into_iter().collect();
    if names.contains("vchordrq") {
        return Ok(DbVectorExtension::VectorChord);
    }
    if names.contains("vector") {
        return Ok(DbVectorExtension::PgVector);
    }

    Err("No vector extension found. Available extensions: vector, vchordrq".into())
}

fn clip_index_query(extension: &DbVectorExtension) -> String {
    match extension {
        DbVectorExtension::VectorChord => r#"
            CREATE INDEX IF NOT EXISTS clip_index ON smart_search
            USING vchordrq (embedding vector_cosine_ops) WITH (options = $$
            residual_quantization = false
            [build.internal]
            lists = [1]
            spherical_centroids = true
            build_threads = 4
            sampling_factor = 1024
            $$)
            "#
        .to_string(),
        DbVectorExtension::PgVector | DbVectorExtension::PgvectoRs => r#"
            CREATE INDEX IF NOT EXISTS clip_index ON smart_search
            USING hnsw (embedding vector_cosine_ops)
            WITH (ef_construction = 300, m = 16)
            "#
        .to_string(),
    }
}

pub async fn set_dimension_size(
    pool: &Pool<Postgres>,
    dim_size: i32,
    vector_extension: Option<DbVectorExtension>,
) -> Result<(), String> {
    if !(1..=65536).contains(&dim_size) {
        return Err(format!("Invalid CLIP dimension size: {dim_size}"));
    }

    let mut tx = pool.begin().await.map_err(|err| err.to_string())?;
    sqlx::query("DELETE FROM smart_search")
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    sqlx::query("ALTER TABLE smart_search DROP CONSTRAINT IF EXISTS dim_size_constraint")
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    sqlx::query(&format!(
        "ALTER TABLE smart_search ADD CONSTRAINT dim_size_constraint CHECK (array_length(embedding::real[], 1) = {dim_size})"
    ))
    .execute(&mut *tx)
    .await
    .map_err(|err| err.to_string())?;
    tx.commit().await.map_err(|err| err.to_string())?;

    let extension = resolve_vector_extension(pool, vector_extension).await?;
    let index_sql = clip_index_query(&extension);

    let mut tx = pool.begin().await.map_err(|err| err.to_string())?;
    sqlx::query("DROP INDEX IF EXISTS clip_index")
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    sqlx::query(&format!(
        "ALTER TABLE smart_search ALTER COLUMN embedding TYPE vector({dim_size})"
    ))
    .execute(&mut *tx)
    .await
    .map_err(|err| err.to_string())?;
    sqlx::query(&index_sql)
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    sqlx::query("ALTER TABLE smart_search DROP CONSTRAINT IF EXISTS dim_size_constraint")
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    tx.commit().await.map_err(|err| err.to_string())?;

    sqlx::query("VACUUM ANALYZE smart_search")
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;

    Ok(())
}

pub async fn delete_all_search_embeddings(pool: &Pool<Postgres>) -> Result<(), String> {
    sqlx::query("TRUNCATE smart_search")
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
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
