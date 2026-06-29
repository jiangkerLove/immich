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
    pub deleted_at: Option<DateTime<Utc>>,
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
                l."deletedAt" as deleted_at,
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
                l."deletedAt" as deleted_at,
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

pub async fn list_all_with_deleted(pool: &Pool<Postgres>) -> Result<Vec<LibraryRow>, sqlx::Error> {
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
                l."deletedAt" as deleted_at,
                COALESCE((
                    SELECT COUNT(*)
                    FROM asset a
                    WHERE a."libraryId" = l.id AND a."deletedAt" IS NULL
                ), 0) as asset_count
            FROM library l
            ORDER BY l."createdAt" ASC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn list_deleted(pool: &Pool<Postgres>) -> Result<Vec<LibraryRow>, sqlx::Error> {
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
                l."deletedAt" as deleted_at,
                COALESCE((
                    SELECT COUNT(*)
                    FROM asset a
                    WHERE a."libraryId" = l.id AND a."deletedAt" IS NULL
                ), 0) as asset_count
            FROM library l
            WHERE l."deletedAt" IS NOT NULL
            ORDER BY l."createdAt" ASC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn hard_delete(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM library WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_refreshed_at(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE library
            SET "refreshedAt" = NOW(), "updatedAt" = NOW()
            WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn detect_offline_external_assets(
    pool: &Pool<Postgres>,
    library_id: &Uuid,
    import_paths: &[String],
    exclusion_patterns: &[String],
) -> Result<u64, sqlx::Error> {
    if import_paths.is_empty() {
        return Ok(0);
    }

    let import_likes: Vec<String> = import_paths
        .iter()
        .map(|path| format!("{}%", path.trim_end_matches('/')))
        .collect();
    let exclusion_likes: Vec<String> = exclusion_patterns
        .iter()
        .map(|pattern| crate::utils::glob::glob_to_sql_like(pattern))
        .collect();

    let mut query = String::from(
        r#"
            UPDATE asset
            SET "isOffline" = true,
                "deletedAt" = NOW(),
                "updatedAt" = NOW()
            WHERE "isOffline" = false
              AND "isExternal" = true
              AND "libraryId" = $1
              AND (
        "#,
    );

    query.push_str("NOT (");
    for (index, _) in import_likes.iter().enumerate() {
        if index > 0 {
            query.push_str(" OR ");
        }
        query.push_str(&format!(r#""originalPath" LIKE ${}"#, index + 2));
    }
    query.push(')');

    if !exclusion_likes.is_empty() {
        query.push_str(" OR ");
        for (index, _) in exclusion_likes.iter().enumerate() {
            if index > 0 {
                query.push_str(" OR ");
            }
            query.push_str(&format!(
                r#""originalPath" LIKE ${}"#,
                import_likes.len() + index + 2
            ));
        }
    }

    query.push(')');

    let mut q = sqlx::query(&query).bind(library_id);
    for like in &import_likes {
        q = q.bind(like);
    }
    for like in &exclusion_likes {
        q = q.bind(like);
    }

    let result = q.execute(pool).await?;
    Ok(result.rows_affected())
}

pub async fn get_asset_id_by_library_path(
    pool: &Pool<Postgres>,
    library_id: &Uuid,
    original_path: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT id
        FROM asset
        WHERE "libraryId" = $1
          AND "originalPath" = $2
        LIMIT 1
        "#,
    )
    .bind(library_id)
    .bind(original_path)
    .fetch_optional(pool)
    .await
}

pub async fn remove_asset_by_id(pool: &Pool<Postgres>, asset_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM asset WHERE id = $1"#)
        .bind(asset_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
pub struct LibraryAssetSyncRow {
    pub id: Uuid,
    pub original_path: String,
    pub file_modified_at: chrono::DateTime<chrono::Utc>,
    pub is_offline: bool,
    pub status: String,
}

pub async fn list_assets_for_sync(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
) -> Result<Vec<LibraryAssetSyncRow>, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(Vec::new());
    }

    sqlx::query_as::<_, LibraryAssetSyncRow>(
        r#"
            SELECT
                id,
                "originalPath" as original_path,
                "fileModifiedAt" as file_modified_at,
                "isOffline" as is_offline,
                status::text as status
            FROM asset
            WHERE id = ANY($1)
        "#,
    )
    .bind(asset_ids)
    .fetch_all(pool)
    .await
}

pub async fn mark_assets_offline(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
    trashed: bool,
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    if trashed {
        sqlx::query(
            r#"
                UPDATE asset
                SET "isOffline" = true, "updatedAt" = NOW()
                WHERE id = ANY($1)
            "#,
        )
        .bind(asset_ids)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            r#"
                UPDATE asset
                SET "isOffline" = true,
                    "deletedAt" = NOW(),
                    "updatedAt" = NOW()
                WHERE id = ANY($1)
            "#,
        )
        .bind(asset_ids)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn mark_assets_online(
    pool: &Pool<Postgres>,
    asset_ids: &[Uuid],
    trashed: bool,
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    if trashed {
        sqlx::query(
            r#"
                UPDATE asset
                SET "isOffline" = false, "updatedAt" = NOW()
                WHERE id = ANY($1)
            "#,
        )
        .bind(asset_ids)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            r#"
                UPDATE asset
                SET "isOffline" = false,
                    "deletedAt" = NULL,
                    "updatedAt" = NOW()
                WHERE id = ANY($1)
            "#,
        )
        .bind(asset_ids)
        .execute(pool)
        .await?;
    }
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
