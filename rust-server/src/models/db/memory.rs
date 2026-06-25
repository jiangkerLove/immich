use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, Pool, Postgres, QueryBuilder};
use tokio::sync::OnceCell;
use uuid::Uuid;

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

    if filter.is_trashed == Some(true) {
        query.push(r#" AND "deletedAt" IS NOT NULL "#);
    } else {
        query.push(r#" AND "deletedAt" IS NULL "#);
    }

    if filter.order.as_deref() == Some("random") {
        query.push(" ORDER BY RANDOM() ");
    } else if filter.order.as_deref() == Some("asc") {
        query.push(r#" ORDER BY "memoryAt" ASC "#);
    } else {
        query.push(r#" ORDER BY "memoryAt" DESC "#);
    }

    if let Some(size) = filter.size {
        query.push(" LIMIT ");
        query.push_bind(size);
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
              INNER JOIN person ON person.id = asset_face."personId"
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
