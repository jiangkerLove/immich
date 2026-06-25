use sqlx::{Pool, Postgres};
use uuid::Uuid;

pub async fn upsert_embedding(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    embedding: &str,
) -> Result<(), sqlx::Error> {
    let embedding = normalize_embedding(embedding);
    sqlx::query(
        r#"
        INSERT INTO smart_search ("assetId", embedding)
        VALUES ($1, $2::vector)
        ON CONFLICT ("assetId") DO UPDATE
        SET embedding = EXCLUDED.embedding
        "#,
    )
    .bind(asset_id)
    .bind(&embedding)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_embedding(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<String> =
        sqlx::query_scalar(r#"SELECT embedding::text FROM smart_search WHERE "assetId" = $1"#)
            .bind(asset_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|value| normalize_embedding(&value)))
}

pub fn normalize_embedding(value: &str) -> String {
    value.trim().to_string()
}
