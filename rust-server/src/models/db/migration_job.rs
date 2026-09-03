use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

use crate::models::db::asset_job::AssetFileJobRow;
use crate::models::db::person_schema::PersonSchema;

#[derive(Debug, Clone, FromRow)]
pub struct MigrationAssetRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub files: Option<serde_json::Value>,
}

pub async fn get_for_migration(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<MigrationAssetRow>, sqlx::Error> {
    sqlx::query_as::<_, MigrationAssetRow>(
        r#"
            SELECT
                asset.id,
                asset."ownerId" AS owner_id,
                (
                    SELECT COALESCE(json_agg(row_to_json(f)), '[]'::json)
                    FROM (
                        SELECT
                            af.id,
                            af.path,
                            af.type AS file_type,
                            af."isEdited" AS is_edited,
                            af."isProgressive" AS is_progressive,
                            af."isTransparent" AS is_transparent
                        FROM asset_file af
                        WHERE af."assetId" = asset.id
                    ) f
                ) AS files
            FROM asset
            WHERE asset.id = $1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn stream_for_migration(pool: &Pool<Postgres>) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT id
            FROM asset
            WHERE "deletedAt" IS NULL
        "#,
    )
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, FromRow)]
pub struct MigrationPersonRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub thumbnail_path: String,
}

pub async fn get_person_for_migration(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    person_id: &Uuid,
) -> Result<Option<MigrationPersonRow>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    sqlx::query_as::<_, MigrationPersonRow>(&format!(
        r#"
            SELECT
                {person_id_col},
                "ownerId" AS owner_id,
                "thumbnailPath" AS thumbnail_path
            FROM person
            WHERE {where_id}
        "#,
        person_id_col = schema.person_id_as_id(""),
        where_id = schema.where_owner_and_id("", "$1", "$2"),
    ))
    .bind(owner_id)
    .bind(person_id)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, Clone, FromRow)]
pub struct MigrationPersonIdRow {
    pub id: Uuid,
    pub owner_id: Uuid,
}

pub async fn stream_persons_for_migration(
    pool: &Pool<Postgres>,
) -> Result<Vec<MigrationPersonIdRow>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    sqlx::query_as::<_, MigrationPersonIdRow>(&format!(
        r#"SELECT {person_id}, "ownerId" AS owner_id FROM person"#,
        person_id = schema.person_id_as_id(""),
    ))
    .fetch_all(pool)
    .await
}

pub fn parse_asset_files(value: Option<serde_json::Value>) -> Vec<AssetFileJobRow> {
    value
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub fn find_asset_file<'a>(
    files: &'a [AssetFileJobRow],
    file_type: &str,
    is_edited: bool,
) -> Option<&'a AssetFileJobRow> {
    files
        .iter()
        .find(|file| file.file_type == file_type && file.is_edited == is_edited)
}
