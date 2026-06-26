use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct LibraryRow {
    pub id: Uuid,
    pub name: String,
    pub owner_id: Uuid,
    pub import_paths: Vec<String>,
    pub exclusion_patterns: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub refreshed_at: Option<DateTime<Utc>>,
    pub asset_count: i64,
}

#[derive(Debug, FromRow)]
pub struct LibraryStatsRow {
    pub photos: i64,
    pub videos: i64,
    pub usage: i64,
}

pub async fn list_all(pool: &Pool<Postgres>) -> Result<Vec<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        r#"
            SELECT
                l.id,
                l.name,
                l."ownerId" as owner_id,
                l."importPaths" as import_paths,
                l."exclusionPatterns" as exclusion_patterns,
                l."createdAt" as created_at,
                l."updatedAt" as updated_at,
                l."refreshedAt" as refreshed_at,
                COALESCE((
                    SELECT COUNT(*)
                    FROM asset a
                    WHERE a."libraryId" = l.id AND a."deletedAt" IS NULL
                ), 0) as asset_count
            FROM library l
            WHERE l."deletedAt" IS NULL
            ORDER BY l."createdAt" ASC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn get_by_id(pool: &Pool<Postgres>, id: &Uuid) -> Result<Option<LibraryRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryRow>(
        r#"
            SELECT
                l.id,
                l.name,
                l."ownerId" as owner_id,
                l."importPaths" as import_paths,
                l."exclusionPatterns" as exclusion_patterns,
                l."createdAt" as created_at,
                l."updatedAt" as updated_at,
                l."refreshedAt" as refreshed_at,
                COALESCE((
                    SELECT COUNT(*)
                    FROM asset a
                    WHERE a."libraryId" = l.id AND a."deletedAt" IS NULL
                ), 0) as asset_count
            FROM library l
            WHERE l.id = $1 AND l."deletedAt" IS NULL
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn create(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    name: &str,
    import_paths: &[String],
    exclusion_patterns: &[String],
) -> Result<LibraryRow, sqlx::Error> {
    let id: Uuid = sqlx::query_scalar(
        r#"
            INSERT INTO library (name, "ownerId", "importPaths", "exclusionPatterns")
            VALUES ($1, $2, $3, $4)
            RETURNING id
        "#,
    )
    .bind(name)
    .bind(owner_id)
    .bind(import_paths)
    .bind(exclusion_patterns)
    .fetch_one(pool)
    .await?;

    get_by_id(pool, &id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn update(
    pool: &Pool<Postgres>,
    id: &Uuid,
    name: Option<&str>,
    import_paths: Option<&[String]>,
    exclusion_patterns: Option<&[String]>,
) -> Result<LibraryRow, sqlx::Error> {
    let current = get_by_id(pool, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;

    let name = name.unwrap_or(&current.name);
    let import_paths = import_paths.unwrap_or(&current.import_paths);
    let exclusion_patterns = exclusion_patterns.unwrap_or(&current.exclusion_patterns);

    sqlx::query(
        r#"
            UPDATE library
            SET name = $2,
                "importPaths" = $3,
                "exclusionPatterns" = $4,
                "updatedAt" = NOW()
            WHERE id = $1 AND "deletedAt" IS NULL
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(import_paths)
    .bind(exclusion_patterns)
    .execute(pool)
    .await?;

    get_by_id(pool, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn soft_delete(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE library
            SET "deletedAt" = NOW(), "updatedAt" = NOW()
            WHERE id = $1 AND "deletedAt" IS NULL
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_statistics(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<LibraryStatsRow>, sqlx::Error> {
    sqlx::query_as::<_, LibraryStatsRow>(
        r#"
            SELECT
                COUNT(*) FILTER (
                    WHERE a.type = 'IMAGE' AND a.visibility != 'hidden'
                ) as photos,
                COUNT(*) FILTER (
                    WHERE a.type = 'VIDEO' AND a.visibility != 'hidden'
                ) as videos,
                COALESCE(SUM(e."fileSizeInByte"), 0) as usage
            FROM library l
            INNER JOIN asset a ON a."libraryId" = l.id AND a."deletedAt" IS NULL
            LEFT JOIN asset_exif e ON e."assetId" = a.id
            WHERE l.id = $1 AND l."deletedAt" IS NULL
            GROUP BY l.id
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}
