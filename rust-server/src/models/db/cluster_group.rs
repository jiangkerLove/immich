use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct ClusterGroupRequestRow {
    pub id: Uuid,
    pub cluster_group_id: Uuid,
    pub user_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub struct ClusterGroupUserRow {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub profile_image_path: String,
    pub avatar_color: Option<String>,
    pub profile_changed_at: DateTime<Utc>,
}

pub async fn create(pool: &Pool<Postgres>) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(r#"INSERT INTO cluster_group DEFAULT VALUES RETURNING id"#)
        .fetch_one(pool)
        .await
}

pub async fn tables_exist(pool: &Pool<Postgres>) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = 'public'
              AND table_name = 'cluster_group_request'
        )
        "#,
    )
    .fetch_one(pool)
    .await
}

pub async fn search_requests(
    pool: &Pool<Postgres>,
    user_id: Option<&Uuid>,
    cluster_group_id: Option<&Uuid>,
) -> Result<Vec<ClusterGroupRequestRow>, sqlx::Error> {
    match (user_id, cluster_group_id) {
        (Some(user_id), None) => {
            sqlx::query_as::<_, ClusterGroupRequestRow>(
                r#"
                SELECT
                    id,
                    "clusterGroupId" AS cluster_group_id,
                    "userId" AS user_id,
                    "createdAt" AS created_at
                FROM cluster_group_request
                WHERE "userId" = $1
                ORDER BY "createdAt" ASC
                "#,
            )
            .bind(user_id)
            .fetch_all(pool)
            .await
        }
        (None, Some(cluster_group_id)) => {
            sqlx::query_as::<_, ClusterGroupRequestRow>(
                r#"
                SELECT
                    id,
                    "clusterGroupId" AS cluster_group_id,
                    "userId" AS user_id,
                    "createdAt" AS created_at
                FROM cluster_group_request
                WHERE "clusterGroupId" = $1
                ORDER BY "createdAt" ASC
                "#,
            )
            .bind(cluster_group_id)
            .fetch_all(pool)
            .await
        }
        _ => Ok(vec![]),
    }
}

pub async fn get_users(
    pool: &Pool<Postgres>,
    cluster_group_id: &Uuid,
    current_user_id: &Uuid,
) -> Result<Vec<ClusterGroupUserRow>, sqlx::Error> {
    sqlx::query_as::<_, ClusterGroupUserRow>(
        r#"
        SELECT
            id,
            email,
            name,
            "profileImagePath" AS profile_image_path,
            "avatarColor" AS avatar_color,
            "profileChangedAt" AS profile_changed_at
        FROM "user"
        WHERE "clusterGroupId" = $1
          AND "deletedAt" IS NULL
        ORDER BY (id = $2) DESC, name ASC
        "#,
    )
    .bind(cluster_group_id)
    .bind(current_user_id)
    .fetch_all(pool)
    .await
}

pub async fn member_cluster_group_ids(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    cluster_group_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if cluster_group_ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_scalar(
        r#"
        SELECT "clusterGroupId"
        FROM "user"
        WHERE id = $1
          AND "clusterGroupId" = ANY($2)
        "#,
    )
    .bind(user_id)
    .bind(cluster_group_ids)
    .fetch_all(pool)
    .await
}

pub async fn invited_cluster_group_ids(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    cluster_group_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if cluster_group_ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_scalar(
        r#"
        SELECT "clusterGroupId"
        FROM cluster_group_request
        WHERE "userId" = $1
          AND "clusterGroupId" = ANY($2)
        "#,
    )
    .bind(user_id)
    .bind(cluster_group_ids)
    .fetch_all(pool)
    .await
}

#[derive(Debug, FromRow)]
struct CreateRequestRow {
    id: Uuid,
    cluster_group_id: Uuid,
    user_id: Uuid,
    created_at: DateTime<Utc>,
    is_inserted: bool,
}

#[derive(Debug)]
pub struct CreateRequestResult {
    pub row: ClusterGroupRequestRow,
    pub is_inserted: bool,
}

pub async fn create_request(
    pool: &Pool<Postgres>,
    cluster_group_id: &Uuid,
    user_id: &Uuid,
) -> Result<CreateRequestResult, sqlx::Error> {
    let row = sqlx::query_as::<_, CreateRequestRow>(
        r#"
        INSERT INTO cluster_group_request ("clusterGroupId", "userId")
        VALUES ($1, $2)
        ON CONFLICT ("clusterGroupId", "userId") DO UPDATE
            SET "clusterGroupId" = EXCLUDED."clusterGroupId"
        RETURNING
            id,
            "clusterGroupId" AS cluster_group_id,
            "userId" AS user_id,
            "createdAt" AS created_at,
            (xmax = 0) AS is_inserted
        "#,
    )
    .bind(cluster_group_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(CreateRequestResult {
        row: ClusterGroupRequestRow {
            id: row.id,
            cluster_group_id: row.cluster_group_id,
            user_id: row.user_id,
            created_at: row.created_at,
        },
        is_inserted: row.is_inserted,
    })
}

pub async fn get_request(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<ClusterGroupRequestRow>, sqlx::Error> {
    sqlx::query_as::<_, ClusterGroupRequestRow>(
        r#"
        SELECT
            id,
            "clusterGroupId" AS cluster_group_id,
            "userId" AS user_id,
            "createdAt" AS created_at
        FROM cluster_group_request
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn delete_request(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM cluster_group_request WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn has_other_members(
    pool: &Pool<Postgres>,
    cluster_group_id: &Uuid,
    user_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM "user"
            WHERE "clusterGroupId" = $1
              AND id != $2
              AND "deletedAt" IS NULL
        )
        "#,
    )
    .bind(cluster_group_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

pub async fn request_owned_by_user(
    pool: &Pool<Postgres>,
    request_id: &Uuid,
    user_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM cluster_group_request
            WHERE id = $1
              AND "userId" = $2
        )
        "#,
    )
    .bind(request_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn can_delete_request(
    pool: &Pool<Postgres>,
    request_id: &Uuid,
    user_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM cluster_group_request
            WHERE id = $1
              AND (
                "userId" = $2
                OR "clusterGroupId" = (
                    SELECT "clusterGroupId" FROM "user" WHERE id = $2
                )
              )
        )
        "#,
    )
    .bind(request_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn user_exists(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM "user" WHERE id = $1 AND "deletedAt" IS NULL
        )
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}
