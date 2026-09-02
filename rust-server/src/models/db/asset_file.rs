use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct AssetFileRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub asset_id: Uuid,
    pub file_type: String,
    pub path: String,
    pub is_edited: bool,
    pub is_progressive: bool,
    pub is_transparent: bool,
}

#[derive(Debug, Default)]
pub struct AssetFileSearchFilter {
    pub asset_id: Uuid,
    pub file_type: Option<String>,
    pub is_edited: Option<bool>,
    pub is_progressive: Option<bool>,
    pub is_transparent: Option<bool>,
}

pub async fn get_by_id(pool: &Pool<Postgres>, id: &Uuid) -> Result<Option<AssetFileRow>, sqlx::Error> {
    sqlx::query_as::<_, AssetFileRow>(
        r#"
        SELECT
            id,
            "createdAt" AS created_at,
            "updatedAt" AS updated_at,
            "assetId" AS asset_id,
            type AS file_type,
            path,
            "isEdited" AS is_edited,
            "isProgressive" AS is_progressive,
            "isTransparent" AS is_transparent
        FROM asset_file
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn search(
    pool: &Pool<Postgres>,
    filter: &AssetFileSearchFilter,
) -> Result<Vec<AssetFileRow>, sqlx::Error> {
    let mut query = QueryBuilder::new(
        r#"
        SELECT
            id,
            "createdAt" AS created_at,
            "updatedAt" AS updated_at,
            "assetId" AS asset_id,
            type AS file_type,
            path,
            "isEdited" AS is_edited,
            "isProgressive" AS is_progressive,
            "isTransparent" AS is_transparent
        FROM asset_file
        WHERE "assetId" =
        "#,
    );
    query.push_bind(filter.asset_id);

    if let Some(file_type) = &filter.file_type {
        query.push(" AND type = ");
        query.push_bind(file_type.clone());
    }
    if let Some(is_edited) = filter.is_edited {
        query.push(r#" AND "isEdited" = "#);
        query.push_bind(is_edited);
    }
    if let Some(is_progressive) = filter.is_progressive {
        query.push(r#" AND "isProgressive" = "#);
        query.push_bind(is_progressive);
    }
    if let Some(is_transparent) = filter.is_transparent {
        query.push(r#" AND "isTransparent" = "#);
        query.push_bind(is_transparent);
    }

    query.build_query_as::<AssetFileRow>().fetch_all(pool).await
}

pub async fn delete_by_id(pool: &Pool<Postgres>, id: &Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(r#"DELETE FROM asset_file WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn filter_owner_accessible_ids(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    file_ids: &[Uuid],
    elevated: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if file_ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_scalar(
        r#"
        SELECT asset_file.id
        FROM asset_file
        INNER JOIN asset ON asset.id = asset_file."assetId"
        WHERE asset."ownerId" = $1
          AND ($2 OR asset.visibility != 'locked')
          AND asset_file.id = ANY($3)
        "#,
    )
    .bind(user_id)
    .bind(elevated)
    .bind(file_ids)
    .fetch_all(pool)
    .await
}
