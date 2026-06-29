use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct MoveHistoryRow {
    pub id: Uuid,
    pub entity_id: Uuid,
    pub path_type: String,
    pub old_path: String,
    pub new_path: String,
}

pub async fn get_by_entity(
    pool: &Pool<Postgres>,
    entity_id: &Uuid,
    path_type: &str,
) -> Result<Option<MoveHistoryRow>, sqlx::Error> {
    sqlx::query_as::<_, MoveHistoryRow>(
        r#"
            SELECT
                id,
                "entityId" AS entity_id,
                "pathType" AS path_type,
                "oldPath" AS old_path,
                "newPath" AS new_path
            FROM move_history
            WHERE "entityId" = $1
              AND "pathType" = $2
        "#,
    )
    .bind(entity_id)
    .bind(path_type)
    .fetch_optional(pool)
    .await
}

pub async fn create(
    pool: &Pool<Postgres>,
    entity_id: &Uuid,
    path_type: &str,
    old_path: &str,
    new_path: &str,
) -> Result<MoveHistoryRow, sqlx::Error> {
    sqlx::query_as::<_, MoveHistoryRow>(
        r#"
            INSERT INTO move_history ("entityId", "pathType", "oldPath", "newPath")
            VALUES ($1, $2, $3, $4)
            RETURNING
                id,
                "entityId" AS entity_id,
                "pathType" AS path_type,
                "oldPath" AS old_path,
                "newPath" AS new_path
        "#,
    )
    .bind(entity_id)
    .bind(path_type)
    .bind(old_path)
    .bind(new_path)
    .fetch_one(pool)
    .await
}

pub async fn update_paths(
    pool: &Pool<Postgres>,
    id: &Uuid,
    old_path: &str,
    new_path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            UPDATE move_history
            SET "oldPath" = $2, "newPath" = $3
            WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(old_path)
    .bind(new_path)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_by_id(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM move_history WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn clean_move_history(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            DELETE FROM move_history
            WHERE "pathType" = 'original'
              AND "entityId" NOT IN (SELECT id FROM asset)
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clean_move_history_single(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            DELETE FROM move_history
            WHERE "pathType" = 'original'
              AND "entityId" = $1
        "#,
    )
    .bind(asset_id)
    .execute(pool)
    .await?;
    Ok(())
}
