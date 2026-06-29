use sqlx::{Pool, Postgres};

pub fn escape_path_prefix(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    let mut pattern = String::from("^");
    for ch in trimmed.chars() {
        match ch {
            '-' | '[' | ']' | '{' | '}' | '(' | ')' | '*' | '+' | '?' | '.' | ',' | '\\' | '^'
            | '$' | '|' | '#' | ' ' => {
                pattern.push('\\');
                pattern.push(ch);
            }
            _ => pattern.push(ch),
        }
    }
    pattern
}

pub async fn sample_file_paths(pool: &Pool<Postgres>) -> Result<Vec<String>, sqlx::Error> {
    let mut paths = Vec::new();

    let assets: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT "originalPath"
        FROM asset
        WHERE "deletedAt" IS NULL
        ORDER BY "createdAt" DESC
        LIMIT 3
        "#,
    )
    .fetch_all(pool)
    .await?;
    paths.extend(assets);

    let people: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT "thumbnailPath"
        FROM person
        WHERE "thumbnailPath" <> ''
        ORDER BY "createdAt" DESC
        LIMIT 3
        "#,
    )
    .fetch_all(pool)
    .await?;
    paths.extend(people);

    let users: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT "profileImagePath"
        FROM "user"
        WHERE "profileImagePath" <> ''
        ORDER BY "createdAt" DESC
        LIMIT 3
        "#,
    )
    .fetch_all(pool)
    .await?;
    paths.extend(users);

    Ok(paths)
}

pub async fn migrate_file_paths(
    pool: &Pool<Postgres>,
    source_folder: &str,
    target_folder: &str,
) -> Result<u64, sqlx::Error> {
    let mut source = source_folder.trim_end_matches('/').to_string();
    if source.starts_with("./") {
        source = source[2..].to_string();
    }
    let target = target_folder.trim_end_matches('/');
    let pattern = escape_path_prefix(&source);

    let mut tx = pool.begin().await?;
    let mut updated = 0u64;

    updated += sqlx::query(
        r#"
        UPDATE asset
        SET "originalPath" = REGEXP_REPLACE("originalPath", $1, $2, 'g')
        WHERE "originalPath" ~ $1
        "#,
    )
    .bind(&pattern)
    .bind(target)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    updated += sqlx::query(
        r#"
        UPDATE asset_file
        SET path = REGEXP_REPLACE(path, $1, $2, 'g')
        WHERE path ~ $1
        "#,
    )
    .bind(&pattern)
    .bind(target)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    updated += sqlx::query(
        r#"
        UPDATE person
        SET "thumbnailPath" = REGEXP_REPLACE("thumbnailPath", $1, $2, 'g')
        WHERE "thumbnailPath" ~ $1
        "#,
    )
    .bind(&pattern)
    .bind(target)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    updated += sqlx::query(
        r#"
        UPDATE "user"
        SET "profileImagePath" = REGEXP_REPLACE("profileImagePath", $1, $2, 'g')
        WHERE "profileImagePath" ~ $1
        "#,
    )
    .bind(&pattern)
    .bind(target)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;
    Ok(updated)
}
