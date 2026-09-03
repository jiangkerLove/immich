use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::{FromRow, Pool, Postgres, QueryBuilder};
use tokio::sync::OnceCell;
use uuid::Uuid;

use super::person_schema::PersonSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryTableKind {
    Modern,
    Legacy,
    Missing,
}

#[derive(Debug, Clone, Copy)]
struct MemoryTables {
    kind: MemoryTableKind,
    memory_table: &'static str,
    asset_link_table: &'static str,
    asset_id_column: &'static str,
}

static MEMORY_TABLES: OnceCell<MemoryTables> = OnceCell::const_new();

async fn resolve_memory_tables(pool: &Pool<Postgres>) -> MemoryTables {
    *MEMORY_TABLES
        .get_or_init(|| async {
            let modern: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = 'public' AND table_name = 'memory'
                )
                "#,
            )
            .fetch_one(pool)
            .await
            .unwrap_or(false);

            if modern {
                return MemoryTables {
                    kind: MemoryTableKind::Modern,
                    memory_table: "memory",
                    asset_link_table: "memory_asset",
                    asset_id_column: r#""assetId""#,
                };
            }

            let legacy: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = 'public' AND table_name = 'memories'
                )
                "#,
            )
            .fetch_one(pool)
            .await
            .unwrap_or(false);

            if legacy {
                return MemoryTables {
                    kind: MemoryTableKind::Legacy,
                    memory_table: "memories",
                    asset_link_table: "memories_assets_assets",
                    asset_id_column: r#""assetsId""#,
                };
            }

            MemoryTables {
                kind: MemoryTableKind::Missing,
                memory_table: "memory",
                asset_link_table: "memory_asset",
                asset_id_column: r#""assetId""#,
            }
        })
        .await
}

#[derive(Debug, FromRow)]
pub struct MemoryRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub owner_id: Uuid,
    pub memory_type: String,
    pub data: Value,
    pub is_saved: bool,
    pub memory_at: DateTime<Utc>,
    pub seen_at: Option<DateTime<Utc>>,
    pub show_at: Option<DateTime<Utc>>,
    pub hide_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
pub struct MemoryAssetRow {
    pub memory_id: Uuid,
    pub asset_id: Uuid,
}

#[derive(Debug, Default)]
pub struct MemorySearchFilter {
    pub for_date: Option<DateTime<Utc>>,
    pub is_trashed: Option<bool>,
    pub is_saved: Option<bool>,
    pub memory_type: Option<String>,
    pub size: Option<i64>,
    pub page: Option<i64>,
    pub is_upcoming: Option<bool>,
    pub order: Option<String>,
}

pub async fn search_memories(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    filter: &MemorySearchFilter,
) -> Result<Vec<MemoryRow>, sqlx::Error> {
    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Ok(vec![]);
    }

    let mut query = QueryBuilder::new(format!(
        r#"
        SELECT
            id,
            "createdAt" AS created_at,
            "updatedAt" AS updated_at,
            "deletedAt" AS deleted_at,
            "ownerId" AS owner_id,
            type AS memory_type,
            data,
            "isSaved" AS is_saved,
            "memoryAt" AS memory_at,
            "seenAt" AS seen_at,
            "showAt" AS show_at,
            "hideAt" AS hide_at
        FROM {}
        WHERE "ownerId" =
        "#,
        tables.memory_table
    ));
    query.push_bind(owner_id);

    append_search_filters(&mut query, filter);

    if filter.order.as_deref() == Some("random") {
        query.push(" ORDER BY RANDOM() ");
    } else if filter.order.as_deref() == Some("asc") {
        query.push(r#" ORDER BY "showAt" ASC NULLS LAST, "memoryAt" ASC "#);
    } else {
        query.push(r#" ORDER BY "showAt" DESC NULLS LAST, "memoryAt" DESC "#);
    }

    if let Some(size) = filter.size {
        query.push(" LIMIT ");
        query.push_bind(size);
    }

    if let (Some(page), Some(size)) = (filter.page, filter.size) {
        if page > 1 {
            query.push(" OFFSET ");
            query.push_bind((page - 1) * size);
        }
    }

    query.build_query_as::<MemoryRow>().fetch_all(pool).await
}

pub async fn get_memory_assets(
    pool: &Pool<Postgres>,
    memory_ids: &[Uuid],
) -> Result<Vec<MemoryAssetRow>, sqlx::Error> {
    if memory_ids.is_empty() {
        return Ok(vec![]);
    }

    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Ok(vec![]);
    }

    let schema = PersonSchema::get(pool).await?;
    let join = schema.join_person_to_face("person", "asset_face");

    let sql = format!(
        r#"
        SELECT
            ma."memoriesId" AS memory_id,
            ma.{asset_id_column} AS asset_id
        FROM {asset_link_table} ma
        INNER JOIN asset ON asset.id = ma.{asset_id_column}
        WHERE ma."memoriesId" = ANY($1)
          AND asset.visibility = 'timeline'
          AND asset."deletedAt" IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM asset_face
              INNER JOIN person ON {join}
              WHERE asset_face."assetId" = asset.id
                AND person."isHidden" = TRUE
          )
        ORDER BY asset."fileCreatedAt" ASC
        "#,
        asset_id_column = tables.asset_id_column,
        asset_link_table = tables.asset_link_table,
    );

    sqlx::query_as::<_, MemoryAssetRow>(&sql)
        .bind(memory_ids)
        .fetch_all(pool)
        .await
}

#[derive(Debug)]
pub struct MemoryCreateData {
    pub owner_id: Uuid,
    pub memory_type: String,
    pub data: Value,
    pub is_saved: bool,
    pub memory_at: DateTime<Utc>,
    pub seen_at: Option<DateTime<Utc>>,
    pub show_at: Option<DateTime<Utc>>,
    pub hide_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
pub struct MemoryUpdateData {
    pub is_saved: Option<bool>,
    pub memory_at: Option<DateTime<Utc>>,
    pub seen_at: Option<DateTime<Utc>>,
}

fn append_search_filters<'a>(
    query: &mut QueryBuilder<'a, Postgres>,
    filter: &MemorySearchFilter,
) {
    if let Some(for_date) = filter.for_date {
        query.push(
            r#"
            AND ("showAt" IS NULL OR "showAt" <= "#,
        );
        query.push_bind(for_date);
        query.push(
            r#")
            AND ("hideAt" IS NULL OR "hideAt" >= "#,
        );
        query.push_bind(for_date);
        query.push(") ");
    }

    if let Some(is_saved) = filter.is_saved {
        query.push(r#" AND "isSaved" = "#);
        query.push_bind(is_saved);
    }

    if let Some(memory_type) = &filter.memory_type {
        query.push(" AND type = ");
        query.push_bind(memory_type.clone());
    }

    if let Some(is_upcoming) = filter.is_upcoming {
        if is_upcoming {
            query.push(r#" AND "showAt" > now() "#);
        } else {
            query.push(r#" AND ("showAt" IS NULL OR "showAt" <= now()) "#);
        }
    }

    if filter.is_trashed == Some(true) {
        query.push(r#" AND "deletedAt" IS NOT NULL "#);
    } else {
        query.push(r#" AND "deletedAt" IS NULL "#);
    }
}

pub async fn owner_has_memory(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    memory_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Ok(false);
    }

    let sql = format!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM {memory_table}
            WHERE id = $1 AND "ownerId" = $2 AND "deletedAt" IS NULL
        )
        "#,
        memory_table = tables.memory_table,
    );

    sqlx::query_scalar(&sql)
        .bind(memory_id)
        .bind(owner_id)
        .fetch_one(pool)
        .await
}

pub async fn get_by_id(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<MemoryRow>, sqlx::Error> {
    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Ok(None);
    }

    let sql = format!(
        r#"
        SELECT
            id,
            "createdAt" AS created_at,
            "updatedAt" AS updated_at,
            "deletedAt" AS deleted_at,
            "ownerId" AS owner_id,
            type AS memory_type,
            data,
            "isSaved" AS is_saved,
            "memoryAt" AS memory_at,
            "seenAt" AS seen_at,
            "showAt" AS show_at,
            "hideAt" AS hide_at
        FROM {memory_table}
        WHERE id = $1 AND "deletedAt" IS NULL
        "#,
        memory_table = tables.memory_table,
    );

    sqlx::query_as::<_, MemoryRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn count_memories(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    filter: &MemorySearchFilter,
) -> Result<i64, sqlx::Error> {
    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Ok(0);
    }

    let mut query = QueryBuilder::new(format!(
        r#"SELECT COUNT(*)::bigint FROM {} WHERE "ownerId" = "#,
        tables.memory_table
    ));
    query.push_bind(owner_id);
    append_search_filters(&mut query, filter);

    query.build_query_scalar().fetch_one(pool).await
}

pub async fn create(
    pool: &Pool<Postgres>,
    data: &MemoryCreateData,
    asset_ids: &[Uuid],
) -> Result<Uuid, sqlx::Error> {
    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Err(sqlx::Error::RowNotFound);
    }

    let mut tx = pool.begin().await?;

    let memory_id: Uuid = sqlx::query_scalar(&format!(
        r#"
        INSERT INTO {memory_table}
            ("ownerId", type, data, "isSaved", "memoryAt", "seenAt", "showAt", "hideAt")
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id
        "#,
        memory_table = tables.memory_table,
    ))
    .bind(data.owner_id)
    .bind(&data.memory_type)
    .bind(&data.data)
    .bind(data.is_saved)
    .bind(data.memory_at)
    .bind(data.seen_at)
    .bind(data.show_at)
    .bind(data.hide_at)
    .fetch_one(&mut *tx)
    .await?;

    if !asset_ids.is_empty() {
        let mut builder = QueryBuilder::new(format!(
            r#"INSERT INTO {} ("memoriesId", "assetId") "#,
            tables.asset_link_table
        ));
        builder.push_values(asset_ids, |mut row, asset_id| {
            row.push_bind(memory_id).push_bind(asset_id);
        });
        builder.build().execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(memory_id)
}

pub async fn update(
    pool: &Pool<Postgres>,
    id: &Uuid,
    data: &MemoryUpdateData,
) -> Result<(), sqlx::Error> {
    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Err(sqlx::Error::RowNotFound);
    }

    let mut query = QueryBuilder::new(format!(
        r#"UPDATE {} SET "#,
        tables.memory_table
    ));
    let mut separated = query.separated(", ");

    if let Some(is_saved) = data.is_saved {
        separated.push(r#""isSaved" = "#);
        separated.push_bind_unseparated(is_saved);
    }
    if let Some(memory_at) = data.memory_at {
        separated.push(r#""memoryAt" = "#);
        separated.push_bind_unseparated(memory_at);
    }
    if let Some(seen_at) = data.seen_at {
        separated.push(r#""seenAt" = "#);
        separated.push_bind_unseparated(seen_at);
    }

    query.push(" WHERE id = ");
    query.push_bind(id);

    query.build().execute(pool).await?;
    Ok(())
}

pub async fn touch_updated_at(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Ok(());
    }

    let sql = format!(
        r#"UPDATE {} SET "updatedAt" = now() WHERE id = $1"#,
        tables.memory_table
    );
    sqlx::query(&sql).bind(id).execute(pool).await?;
    Ok(())
}

pub async fn delete(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Err(sqlx::Error::RowNotFound);
    }

    let sql = format!(r#"DELETE FROM {} WHERE id = $1"#, tables.memory_table);
    sqlx::query(&sql).bind(id).execute(pool).await?;
    Ok(())
}

pub async fn filter_asset_ids_in_memory(
    pool: &Pool<Postgres>,
    memory_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(vec![]);
    }

    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Ok(vec![]);
    }

    let sql = format!(
        r#"
        SELECT {asset_id_column} AS asset_id
        FROM {asset_link_table}
        WHERE "memoriesId" = $1 AND {asset_id_column} = ANY($2)
        "#,
        asset_id_column = tables.asset_id_column,
        asset_link_table = tables.asset_link_table,
    );

    #[derive(FromRow)]
    struct Row {
        asset_id: Uuid,
    }

    let rows = sqlx::query_as::<_, Row>(&sql)
        .bind(memory_id)
        .bind(asset_ids)
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(|row| row.asset_id).collect())
}

pub async fn add_asset_ids(
    pool: &Pool<Postgres>,
    memory_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Err(sqlx::Error::RowNotFound);
    }

    let mut builder = QueryBuilder::new(format!(
        r#"INSERT INTO {} ("memoriesId", "assetId") "#,
        tables.asset_link_table
    ));
    builder.push_values(asset_ids, |mut row, asset_id| {
        row.push_bind(memory_id).push_bind(asset_id);
    });
    builder.build().execute(pool).await?;
    Ok(())
}

pub async fn remove_asset_ids(
    pool: &Pool<Postgres>,
    memory_id: &Uuid,
    asset_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Err(sqlx::Error::RowNotFound);
    }

    let sql = format!(
        r#"
        DELETE FROM {asset_link_table}
        WHERE "memoriesId" = $1 AND {asset_id_column} = ANY($2)
        "#,
        asset_link_table = tables.asset_link_table,
        asset_id_column = tables.asset_id_column,
    );

    sqlx::query(&sql)
        .bind(memory_id)
        .bind(asset_ids)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn cleanup_stale(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    let tables = resolve_memory_tables(pool).await;
    if tables.kind == MemoryTableKind::Missing {
        return Ok(());
    }

    let delete_assets_sql = format!(
        r#"
            DELETE FROM {asset_link} ma
            USING asset
            WHERE ma."{asset_id_col}" = asset.id
              AND asset.visibility <> 'timeline'::asset_visibility_enum
        "#,
        asset_link = tables.asset_link_table,
        asset_id_col = tables.asset_id_column,
    );
    sqlx::query(&delete_assets_sql).execute(pool).await?;

    let delete_memories_sql = format!(
        r#"
            DELETE FROM {memory_table}
            WHERE "createdAt" < now() - interval '30 days'
              AND "isSaved" = false
        "#,
        memory_table = tables.memory_table,
    );
    sqlx::query(&delete_memories_sql).execute(pool).await?;
    Ok(())
}

#[derive(Debug)]
pub struct DayOfYearGroup {
    pub year: i32,
    pub asset_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
struct DayOfYearAssetJson {
    id: Uuid,
}

#[derive(Debug, sqlx::FromRow)]
struct DayOfYearRow {
    year: i32,
    assets: Option<serde_json::Value>,
}

pub async fn get_assets_by_day_of_year(
    pool: &Pool<Postgres>,
    owner_ids: &[Uuid],
    month: i32,
    day: i32,
    year: i32,
) -> Result<Vec<DayOfYearGroup>, sqlx::Error> {
    if owner_ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query_as::<_, DayOfYearRow>(
        r#"
            WITH res AS (
                WITH today AS (
                    SELECT make_date(year::int, $1::int, $2::int) AS date
                    FROM generate_series(
                        (
                            SELECT date_part(
                                'year',
                                min(("localDateTime" AT TIME ZONE 'UTC')::date)
                            )::int
                            FROM asset
                        ),
                        $3
                    ) AS year
                )
                SELECT a.*
                FROM today
                INNER JOIN LATERAL (
                    SELECT asset.id, asset."localDateTime"
                    FROM asset
                    INNER JOIN asset_job_status ON asset.id = asset_job_status."assetId"
                    WHERE (asset."localDateTime" AT TIME ZONE 'UTC')::date = today.date
                      AND asset."ownerId" = ANY($4::uuid[])
                      AND asset.visibility = 'timeline'::asset_visibility_enum
                      AND EXISTS (
                          SELECT 1
                          FROM asset_file
                          WHERE "assetId" = asset.id
                            AND asset_file.type = 'preview'
                      )
                      AND asset."deletedAt" IS NULL
                    ORDER BY (asset."localDateTime" AT TIME ZONE 'UTC')::date DESC
                    LIMIT 20
                ) AS a ON true
            )
            SELECT
                date_part(
                    'year',
                    ("localDateTime" AT TIME ZONE 'UTC')::date
                )::int AS year,
                json_agg(res) AS assets
            FROM res
            GROUP BY ("localDateTime" AT TIME ZONE 'UTC')::date
            ORDER BY ("localDateTime" AT TIME ZONE 'UTC')::date DESC
        "#,
    )
    .bind(month)
    .bind(day)
    .bind(year - 1)
    .bind(owner_ids)
    .fetch_all(pool)
    .await?;

    let mut groups = Vec::new();
    for row in rows {
        let asset_ids: Vec<Uuid> = row
            .assets
            .and_then(|value| serde_json::from_value::<Vec<DayOfYearAssetJson>>(value).ok())
            .map(|assets| assets.into_iter().map(|asset| asset.id).collect())
            .unwrap_or_default();
        if asset_ids.is_empty() {
            continue;
        }
        groups.push(DayOfYearGroup {
            year: row.year,
            asset_ids,
        });
    }

    Ok(groups)
}
