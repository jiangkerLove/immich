use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct IntegrityReportRow {
    pub id: Uuid,
    pub report_type: String,
    pub path: String,
    pub asset_id: Option<Uuid>,
    pub file_asset_id: Option<Uuid>,
}

#[derive(Debug, FromRow)]
pub struct IntegrityReportSummaryRow {
    pub report_type: String,
    pub count: i64,
}

pub async fn get_by_id(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<IntegrityReportRow, sqlx::Error> {
    sqlx::query_as::<_, IntegrityReportRow>(
        r#"
        SELECT
            id,
            type as report_type,
            path,
            "assetId" as asset_id,
            "fileAssetId" as file_asset_id
        FROM integrity_report
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
}

pub async fn get_summary(
    pool: &Pool<Postgres>,
) -> Result<Vec<IntegrityReportSummaryRow>, sqlx::Error> {
    sqlx::query_as::<_, IntegrityReportSummaryRow>(
        r#"
        SELECT type as report_type, COUNT(*)::bigint as count
        FROM integrity_report
        GROUP BY type
        "#,
    )
    .fetch_all(pool)
    .await
}

#[derive(Debug, FromRow)]
pub struct IntegrityReportListRow {
    pub id: Uuid,
    pub report_type: String,
    pub path: String,
}

pub async fn get_report_page(
    pool: &Pool<Postgres>,
    report_type: &str,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<IntegrityReportListRow>, sqlx::Error> {
    match cursor {
        Some(cursor) => {
            sqlx::query_as::<_, IntegrityReportListRow>(
                r#"
                SELECT id, type as report_type, path
                FROM integrity_report
                WHERE type = $1 AND id <= $2
                ORDER BY id DESC
                LIMIT $3
                "#,
            )
            .bind(report_type)
            .bind(cursor)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query_as::<_, IntegrityReportListRow>(
                r#"
                SELECT id, type as report_type, path
                FROM integrity_report
                WHERE type = $1
                ORDER BY id DESC
                LIMIT $2
                "#,
            )
            .bind(report_type)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
    }
}

#[derive(Debug, FromRow)]
pub struct IntegrityReportCsvRow {
    pub id: Uuid,
    pub report_type: String,
    pub path: String,
    pub asset_id: Option<Uuid>,
    pub file_asset_id: Option<Uuid>,
}

pub async fn stream_report_rows(
    pool: &Pool<Postgres>,
    report_type: &str,
) -> Result<Vec<IntegrityReportCsvRow>, sqlx::Error> {
    sqlx::query_as::<_, IntegrityReportCsvRow>(
        r#"
        SELECT
            id,
            type as report_type,
            path,
            "assetId" as asset_id,
            "fileAssetId" as file_asset_id
        FROM integrity_report
        WHERE type = $1
        ORDER BY "createdAt" DESC
        "#,
    )
    .bind(report_type)
    .fetch_all(pool)
    .await
}

pub async fn delete_by_id(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM integrity_report WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_asset_file(pool: &Pool<Postgres>, file_asset_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM asset_file WHERE id = $1"#)
        .bind(file_asset_id)
        .execute(pool)
        .await?;
    Ok(())
}
