use serde_json::Value;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct AssetEditRow {
    pub id: Uuid,
    pub action: String,
    pub parameters: Value,
}

pub async fn list_by_asset(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Vec<AssetEditRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetEditRow>(
        r#"
            SELECT id, action, parameters
            FROM asset_edit
            WHERE "assetId" = $1
            ORDER BY sequence ASC
        "#,
    )
    .bind(asset_id)
    .fetch_all(pool)
    .await
}

pub async fn replace_all(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    edits: &[(String, Value)],
) -> Result<Vec<AssetEditRow>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(r#"DELETE FROM asset_edit WHERE "assetId" = $1"#)
        .bind(asset_id)
        .execute(&mut *tx)
        .await?;

    let mut results = Vec::new();
    for (sequence, (action, parameters)) in edits.iter().enumerate() {
        let row = sqlx::query_as::<_, AssetEditRow>(
            r#"
                INSERT INTO asset_edit ("assetId", action, parameters, sequence)
                VALUES ($1, $2, $3, $4)
                RETURNING id, action, parameters
            "#,
        )
        .bind(asset_id)
        .bind(action)
        .bind(parameters)
        .bind(sequence as i32)
        .fetch_one(&mut *tx)
        .await?;
        results.push(row);
    }

    sqlx::query(r#"UPDATE asset SET "isEdited" = $1 WHERE id = $2"#)
        .bind(!edits.is_empty())
        .bind(asset_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(results)
}

pub async fn delete_all(pool: &Pool<Postgres>, asset_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM asset_edit WHERE "assetId" = $1"#)
        .bind(asset_id)
        .execute(pool)
        .await?;
    sqlx::query(r#"UPDATE asset SET "isEdited" = false WHERE id = $1"#)
        .bind(asset_id)
        .execute(pool)
        .await?;
    Ok(())
}
