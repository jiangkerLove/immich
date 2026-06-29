use sqlx::{Pool, Postgres};
use uuid::Uuid;

const AUDIT_TABLES: &[&str] = &[
    "album_audit",
    "album_user_audit",
    "album_asset_audit",
    "asset_audit",
    "asset_face_audit",
    "asset_edit_audit",
    "asset_metadata_audit",
    "asset_ocr_audit",
    "memory_audit",
    "memory_asset_audit",
    "partner_audit",
    "person_audit",
    "stack_audit",
    "user_audit",
    "user_metadata_audit",
];

pub async fn delete_empty_tags(pool: &Pool<Postgres>) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
            DELETE FROM tag
            WHERE NOT EXISTS (
                SELECT 1
                FROM tag_closure
                INNER JOIN tag_asset ON tag_asset."tagId" = tag_closure.id_descendant
                WHERE tag_closure.id_ancestor = tag.id
            )
        "#,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn cleanup_audit_tables(
    pool: &Pool<Postgres>,
    days_ago: i64,
) -> Result<u64, sqlx::Error> {
    let mut total = 0u64;
    for table in AUDIT_TABLES {
        let query = format!(
            r#"DELETE FROM {table} WHERE "deletedAt" < NOW() - ($1 * INTERVAL '1 day')"#
        );
        let result = sqlx::query(&query).bind(days_ago).execute(pool).await?;
        total += result.rows_affected();
    }
    Ok(total)
}

#[derive(Debug, sqlx::FromRow)]
pub struct ExpiredHlsSessionRow {
    pub id: Uuid,
    pub owner_id: Uuid,
}

pub async fn list_expired_hls_sessions(
    pool: &Pool<Postgres>,
) -> Result<Vec<ExpiredHlsSessionRow>, sqlx::Error> {
    sqlx::query_as::<_, ExpiredHlsSessionRow>(
        r#"
            SELECT
                video_stream_session.id,
                asset."ownerId" as owner_id
            FROM video_stream_session
            INNER JOIN asset ON asset.id = video_stream_session."assetId"
            WHERE video_stream_session."expiresAt" <= NOW()
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn delete_hls_session(pool: &Pool<Postgres>, session_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM video_stream_session WHERE id = $1"#)
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn sync_all_user_usage(pool: &Pool<Postgres>) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
            UPDATE "user" u
            SET "quotaUsageInBytes" = COALESCE((
                SELECT SUM(e."fileSizeInByte")
                FROM asset a
                LEFT JOIN asset_exif e ON e."assetId" = a.id
                WHERE a."ownerId" = u.id
                  AND a."libraryId" IS NULL
            ), 0),
            "updatedAt" = NOW()
            WHERE u."deletedAt" IS NULL
        "#,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
