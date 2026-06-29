use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetOcrRow {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub x3: f32,
    pub y3: f32,
    pub x4: f32,
    pub y4: f32,
    pub box_score: f32,
    pub text_score: f32,
    pub text: String,
}

pub async fn get_by_asset_id(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Vec<AssetOcrRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetOcrRow>(
        r#"
        SELECT
            id,
            "assetId" as asset_id,
            x1, y1, x2, y2, x3, y3, x4, y4,
            "boxScore" as box_score,
            "textScore" as text_score,
            text
        FROM asset_ocr
        WHERE "assetId" = $1 AND "isVisible" = true
        "#,
    )
    .bind(asset_id)
    .fetch_all(pool)
    .await
}

#[derive(Debug, sqlx::FromRow)]
pub struct OcrVisibilityRow {
    pub id: Uuid,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub x3: f32,
    pub y3: f32,
    pub x4: f32,
    pub y4: f32,
    pub text: String,
    pub is_visible: bool,
}

pub async fn list_for_visibility_by_asset(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Vec<OcrVisibilityRow>, sqlx::Error> {
    sqlx::query_as::<_, OcrVisibilityRow>(
        r#"
        SELECT
            id,
            x1, y1, x2, y2, x3, y3, x4, y4,
            text,
            "isVisible" AS is_visible
        FROM asset_ocr
        WHERE "assetId" = $1
        "#,
    )
    .bind(asset_id)
    .fetch_all(pool)
    .await
}

pub async fn update_visibilities(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    visible_ids: &[Uuid],
    hidden_ids: &[Uuid],
    search_text: &str,
) -> Result<(), sqlx::Error> {
    if visible_ids.is_empty() && hidden_ids.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;

    if !visible_ids.is_empty() {
        sqlx::query(
            r#"
            UPDATE asset_ocr
            SET "isVisible" = true
            WHERE id = ANY($1)
            "#,
        )
        .bind(visible_ids)
        .execute(&mut *tx)
        .await?;
    }

    if !hidden_ids.is_empty() {
        sqlx::query(
            r#"
            UPDATE asset_ocr
            SET "isVisible" = false
            WHERE id = ANY($1)
            "#,
        )
        .bind(hidden_ids)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        r#"
        UPDATE ocr_search
        SET text = $1
        WHERE "assetId" = $2
        "#,
    )
    .bind(search_text)
    .bind(asset_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

#[derive(Debug)]
pub struct OcrInsertRow {
    pub asset_id: Uuid,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub x3: f32,
    pub y3: f32,
    pub x4: f32,
    pub y4: f32,
    pub box_score: f32,
    pub text_score: f32,
    pub text: String,
}

pub async fn delete_all(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("TRUNCATE asset_ocr").execute(&mut *tx).await?;
    sqlx::query("TRUNCATE ocr_search").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn upsert_for_asset(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    rows: &[OcrInsertRow],
    search_text: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(r#"DELETE FROM asset_ocr WHERE "assetId" = $1"#)
        .bind(asset_id)
        .execute(&mut *tx)
        .await?;

    for row in rows {
        sqlx::query(
            r#"
                INSERT INTO asset_ocr (
                    "assetId", x1, y1, x2, y2, x3, y3, x4, y4,
                    "boxScore", "textScore", text
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(asset_id)
        .bind(row.x1)
        .bind(row.y1)
        .bind(row.x2)
        .bind(row.y2)
        .bind(row.x3)
        .bind(row.y3)
        .bind(row.x4)
        .bind(row.y4)
        .bind(row.box_score)
        .bind(row.text_score)
        .bind(&row.text)
        .execute(&mut *tx)
        .await?;
    }

    if rows.is_empty() {
        sqlx::query(r#"DELETE FROM ocr_search WHERE "assetId" = $1"#)
            .bind(asset_id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query(
            r#"
                INSERT INTO ocr_search ("assetId", text)
                VALUES ($1, $2)
                ON CONFLICT ("assetId") DO UPDATE SET text = EXCLUDED.text
            "#,
        )
        .bind(asset_id)
        .bind(search_text)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}
