use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

pub const REPORT_TYPE_UNTRACKED: &str = "untracked_file";
pub const REPORT_TYPE_MISSING: &str = "missing_file";
pub const REPORT_TYPE_CHECKSUM: &str = "checksum_mismatch";

pub const CHECKSUM_CHECKPOINT_KEY: &str = "integrity-checksum-checkpoint";

pub const JOBS_INTEGRITY_BATCH_SIZE: i64 = 10_000;

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

#[derive(Debug)]
pub struct IntegrityReportInsert {
    pub report_type: String,
    pub path: String,
    pub asset_id: Option<Uuid>,
    pub file_asset_id: Option<Uuid>,
}

pub async fn create_reports(
    pool: &Pool<Postgres>,
    reports: &[IntegrityReportInsert],
) -> Result<(), sqlx::Error> {
    for report in reports {
        sqlx::query(
            r#"
            INSERT INTO integrity_report (type, path, "assetId", "fileAssetId")
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (type, path) DO UPDATE SET
                "assetId" = EXCLUDED."assetId",
                "fileAssetId" = EXCLUDED."fileAssetId"
            "#,
        )
        .bind(&report.report_type)
        .bind(&report.path)
        .bind(report.asset_id)
        .bind(report.file_asset_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn delete_by_ids(pool: &Pool<Postgres>, ids: &[Uuid]) -> Result<(), sqlx::Error> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query(r#"DELETE FROM integrity_report WHERE id = ANY($1)"#)
        .bind(ids)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, FromRow)]
pub struct AssetPathRow {
    pub original_path: String,
    pub encoded_video_path: Option<String>,
}

pub async fn get_asset_paths_by_paths(
    pool: &Pool<Postgres>,
    paths: &[String],
) -> Result<Vec<AssetPathRow>, sqlx::Error> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    sqlx::query_as::<_, AssetPathRow>(
        r#"
        SELECT
            a."originalPath" AS original_path,
            af.path AS encoded_video_path
        FROM asset a
        LEFT JOIN asset_file af
            ON af."assetId" = a.id
            AND af.type = 'encoded_video'
        WHERE a."originalPath" = ANY($1)
            OR af.path = ANY($1)
        "#,
    )
    .bind(paths)
    .fetch_all(pool)
    .await
}

#[derive(Debug, FromRow)]
pub struct AssetFilePathRow {
    pub path: String,
}

pub async fn get_asset_file_paths_by_paths(
    pool: &Pool<Postgres>,
    paths: &[String],
) -> Result<Vec<AssetFilePathRow>, sqlx::Error> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    sqlx::query_as::<_, AssetFilePathRow>(
        r#"SELECT path FROM asset_file WHERE path = ANY($1)"#,
    )
    .bind(paths)
    .fetch_all(pool)
    .await
}

#[derive(Debug, FromRow)]
pub struct PersonThumbnailPathRow {
    pub thumbnail_path: String,
}

pub async fn get_person_thumbnail_paths_by_paths(
    pool: &Pool<Postgres>,
    paths: &[String],
) -> Result<Vec<PersonThumbnailPathRow>, sqlx::Error> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    sqlx::query_as::<_, PersonThumbnailPathRow>(
        r#"SELECT "thumbnailPath" AS thumbnail_path FROM person WHERE "thumbnailPath" = ANY($1)"#,
    )
    .bind(paths)
    .fetch_all(pool)
    .await
}

#[derive(Debug, FromRow)]
pub struct TrackedPathRow {
    pub path: String,
}

pub async fn get_tracked_paths(
    pool: &Pool<Postgres>,
    paths: &[String],
) -> Result<Vec<TrackedPathRow>, sqlx::Error> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    sqlx::query_as::<_, TrackedPathRow>(
        r#"
        SELECT "originalPath" AS path
        FROM asset
        WHERE "originalPath" = ANY($1)
        UNION
        SELECT path
        FROM asset_file
        WHERE path = ANY($1)
        UNION
        SELECT "thumbnailPath" AS path
        FROM person
        WHERE "thumbnailPath" = ANY($1)
        "#,
    )
    .bind(paths)
    .fetch_all(pool)
    .await
}

pub async fn get_asset_count(pool: &Pool<Postgres>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(r#"SELECT COUNT(*)::bigint FROM asset"#)
        .fetch_one(pool)
        .await
}

#[derive(Debug, FromRow)]
pub struct AssetPathItemRow {
    pub path: String,
    pub asset_id: Option<Uuid>,
    pub file_asset_id: Option<Uuid>,
    pub report_id: Option<Uuid>,
}

pub async fn stream_asset_paths_page(
    pool: &Pool<Postgres>,
    offset: i64,
    limit: i64,
) -> Result<Vec<AssetPathItemRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetPathItemRow>(
        r#"
        SELECT
            all_paths.path,
            all_paths.asset_id,
            all_paths.file_asset_id,
            ir.id AS report_id
        FROM (
            SELECT
                a."originalPath" AS path,
                a.id AS asset_id,
                NULL::uuid AS file_asset_id
            FROM asset a
            WHERE a."deletedAt" IS NULL
            UNION ALL
            SELECT
                af.path,
                NULL::uuid AS asset_id,
                af.id AS file_asset_id
            FROM asset_file af
        ) AS all_paths
        LEFT JOIN integrity_report ir
            ON ir.type = $1
            AND (
                ir."assetId" = all_paths.asset_id
                OR ir."fileAssetId" = all_paths.file_asset_id
            )
        ORDER BY all_paths.path
        OFFSET $2
        LIMIT $3
        "#,
    )
    .bind(REPORT_TYPE_MISSING)
    .bind(offset)
    .bind(limit)
    .fetch_all(pool)
    .await
}

#[derive(Debug, FromRow)]
pub struct AssetChecksumRow {
    pub original_path: String,
    pub checksum: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub asset_id: Uuid,
    pub report_id: Option<Uuid>,
}

pub async fn stream_asset_checksums_page(
    pool: &Pool<Postgres>,
    start_marker: Option<DateTime<Utc>>,
    end_marker: Option<DateTime<Utc>>,
    offset: i64,
    limit: i64,
) -> Result<Vec<AssetChecksumRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetChecksumRow>(
        r#"
        SELECT
            a."originalPath" AS original_path,
            a.checksum,
            a."createdAt" AS created_at,
            a.id AS asset_id,
            ir.id AS report_id
        FROM asset a
        LEFT JOIN integrity_report ir
            ON ir."assetId" = a.id
            AND ir.type = $1
        WHERE a."deletedAt" IS NULL
            AND a."isExternal" = FALSE
            AND ($2::timestamptz IS NULL OR a."createdAt" >= $2)
            AND ($3::timestamptz IS NULL OR a."createdAt" <= $3)
        ORDER BY a."createdAt" ASC
        OFFSET $4
        LIMIT $5
        "#,
    )
    .bind(REPORT_TYPE_CHECKSUM)
    .bind(start_marker)
    .bind(end_marker)
    .bind(offset)
    .bind(limit)
    .fetch_all(pool)
    .await
}

#[derive(Debug, FromRow)]
pub struct IntegrityReportWithChecksumRow {
    pub report_id: Uuid,
    pub path: String,
    pub checksum: Option<Vec<u8>>,
}

pub async fn stream_integrity_reports_with_checksum_page(
    pool: &Pool<Postgres>,
    report_type: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<IntegrityReportWithChecksumRow>, sqlx::Error> {
    if report_type == REPORT_TYPE_CHECKSUM {
        sqlx::query_as::<_, IntegrityReportWithChecksumRow>(
            r#"
            SELECT
                ir.id AS report_id,
                ir.path,
                a.checksum
            FROM integrity_report ir
            LEFT JOIN asset a ON a."originalPath" = ir.path
            WHERE ir.type = $1
            ORDER BY ir."createdAt" DESC
            OFFSET $2
            LIMIT $3
            "#,
        )
        .bind(report_type)
        .bind(offset)
        .bind(limit)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, IntegrityReportWithChecksumRow>(
            r#"
            SELECT
                ir.id AS report_id,
                ir.path,
                NULL::bytea AS checksum
            FROM integrity_report ir
            WHERE ir.type = $1
            ORDER BY ir."createdAt" DESC
            OFFSET $2
            LIMIT $3
            "#,
        )
        .bind(report_type)
        .bind(offset)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
}

#[derive(Debug, FromRow)]
pub struct IntegrityReportDeleteRow {
    pub id: Uuid,
    pub path: String,
    pub asset_id: Option<Uuid>,
    pub file_asset_id: Option<Uuid>,
}

pub async fn stream_integrity_reports_by_property_page(
    pool: &Pool<Postgres>,
    property: Option<&str>,
    report_type: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<IntegrityReportDeleteRow>, sqlx::Error> {
    match property {
        Some("assetId") => {
            sqlx::query_as::<_, IntegrityReportDeleteRow>(
                r#"
                SELECT id, path, "assetId" AS asset_id, "fileAssetId" AS file_asset_id
                FROM integrity_report
                WHERE ($1::varchar IS NULL OR type = $1)
                    AND "assetId" IS NOT NULL
                ORDER BY id
                OFFSET $2
                LIMIT $3
                "#,
            )
            .bind(report_type)
            .bind(offset)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        Some("fileAssetId") => {
            sqlx::query_as::<_, IntegrityReportDeleteRow>(
                r#"
                SELECT id, path, "assetId" AS asset_id, "fileAssetId" AS file_asset_id
                FROM integrity_report
                WHERE ($1::varchar IS NULL OR type = $1)
                    AND "fileAssetId" IS NOT NULL
                ORDER BY id
                OFFSET $2
                LIMIT $3
                "#,
            )
            .bind(report_type)
            .bind(offset)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        _ => {
            sqlx::query_as::<_, IntegrityReportDeleteRow>(
                r#"
                SELECT id, path, "assetId" AS asset_id, "fileAssetId" AS file_asset_id
                FROM integrity_report
                WHERE ($1::varchar IS NULL OR type = $1)
                    AND "assetId" IS NULL
                    AND "fileAssetId" IS NULL
                ORDER BY id
                OFFSET $2
                LIMIT $3
                "#,
            )
            .bind(report_type)
            .bind(offset)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
    }
}
