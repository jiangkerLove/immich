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
