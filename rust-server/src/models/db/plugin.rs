use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct PluginRow {
    pub id: Uuid,
    pub name: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub templates: Value,
    pub methods: Value,
}

const PLUGIN_SELECT: &str = r#"
    SELECT
        plugin.id,
        plugin.name,
        plugin.title,
        plugin.description,
        plugin.author,
        plugin.version,
        plugin."createdAt" as created_at,
        plugin."updatedAt" as updated_at,
        plugin.templates,
        (
            SELECT COALESCE(json_agg(agg), '[]'::json)
            FROM (
                SELECT
                    plugin_method.name,
                    plugin_method.title,
                    plugin_method.description,
                    plugin_method.types,
                    plugin_method.schema,
                    plugin_method."hostFunctions" as "hostFunctions",
                    plugin_method."uiHints" as "uiHints",
                    plugin.name as "pluginName"
                FROM plugin_method
                WHERE plugin_method."pluginId" = plugin.id
            ) AS agg
        ) AS methods
    FROM plugin
"#;

pub async fn search(
    pool: &Pool<Postgres>,
    id: Option<Uuid>,
    name: Option<&str>,
    title: Option<&str>,
    description: Option<&str>,
    version: Option<&str>,
) -> Result<Vec<PluginRow>, sqlx::Error> {
    let mut query = String::from(PLUGIN_SELECT);
    query.push_str(" WHERE 1=1");
    let mut binds = 0u8;

    if id.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin.id = ${binds}"));
    }
    if name.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin.name = ${binds}"));
    }
    if title.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin.title = ${binds}"));
    }
    if description.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin.description = ${binds}"));
    }
    if version.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin.version = ${binds}"));
    }
    query.push_str(r#" ORDER BY plugin.name"#);

    let mut q = sqlx::query_as::<_, PluginRow>(&query);
    if let Some(id) = id {
        q = q.bind(id);
    }
    if let Some(name) = name {
        q = q.bind(name);
    }
    if let Some(title) = title {
        q = q.bind(title);
    }
    if let Some(description) = description {
        q = q.bind(description);
    }
    if let Some(version) = version {
        q = q.bind(version);
    }
    q.fetch_all(pool).await
}

pub async fn get_by_id(pool: &Pool<Postgres>, id: &Uuid) -> Result<Option<PluginRow>, sqlx::Error> {
    let query = format!("{PLUGIN_SELECT} WHERE plugin.id = $1");
    sqlx::query_as::<_, PluginRow>(&query)
        .bind(id)
        .fetch_optional(pool)
        .await
}

#[derive(Debug, FromRow)]
pub struct PluginMethodRow {
    pub plugin_name: String,
    pub plugin_id: Uuid,
    pub id: Uuid,
    pub name: String,
    pub title: String,
    pub description: String,
    pub types: Vec<String>,
    pub schema: Option<Value>,
    pub host_functions: bool,
    pub ui_hints: Vec<String>,
}

pub async fn search_methods(
    pool: &Pool<Postgres>,
    id: Option<Uuid>,
    name: Option<&str>,
    title: Option<&str>,
    description: Option<&str>,
    workflow_type: Option<&str>,
    plugin_name: Option<&str>,
    plugin_version: Option<&str>,
) -> Result<Vec<PluginMethodRow>, sqlx::Error> {
    let mut query = String::from(
        r#"
        SELECT
            plugin.name as plugin_name,
            plugin_method."pluginId" as plugin_id,
            plugin_method.id,
            plugin_method.name,
            plugin_method.title,
            plugin_method.description,
            plugin_method.types,
            plugin_method.schema,
            plugin_method."hostFunctions" as host_functions,
            plugin_method."uiHints" as ui_hints
        FROM plugin_method
        INNER JOIN plugin ON plugin.id = plugin_method."pluginId"
        WHERE 1=1
        "#,
    );
    let mut binds = 0u8;
    if id.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin_method.id = ${binds}"));
    }
    if name.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin_method.name = ${binds}"));
    }
    if title.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin_method.title = ${binds}"));
    }
    if description.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin_method.description = ${binds}"));
    }
    if workflow_type.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin_method.types @> ARRAY[${binds}]::varchar[]"));
    }
    if plugin_name.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin.name = ${binds}"));
    }
    if plugin_version.is_some() {
        binds += 1;
        query.push_str(&format!(" AND plugin.version = ${binds}"));
    }
    query.push_str(" ORDER BY plugin_method.name");

    let mut q = sqlx::query_as::<_, PluginMethodRow>(&query);
    if let Some(id) = id {
        q = q.bind(id);
    }
    if let Some(name) = name {
        q = q.bind(name);
    }
    if let Some(title) = title {
        q = q.bind(title);
    }
    if let Some(description) = description {
        q = q.bind(description);
    }
    if let Some(workflow_type) = workflow_type {
        q = q.bind(workflow_type);
    }
    if let Some(plugin_name) = plugin_name {
        q = q.bind(plugin_name);
    }
    if let Some(plugin_version) = plugin_version {
        q = q.bind(plugin_version);
    }
    q.fetch_all(pool).await
}

#[derive(Debug, FromRow)]
pub struct PluginMethodValidationRow {
    pub id: Uuid,
    pub name: String,
    pub plugin_name: String,
    pub types: Vec<String>,
}

pub async fn get_for_validation(
    pool: &Pool<Postgres>,
) -> Result<Vec<PluginMethodValidationRow>, sqlx::Error> {
    sqlx::query_as::<_, PluginMethodValidationRow>(
        r#"
        SELECT
            plugin_method.id,
            plugin_method.name,
            plugin.name as plugin_name,
            plugin_method.types
        FROM plugin_method
        INNER JOIN plugin ON plugin_method."pluginId" = plugin.id
        "#,
    )
    .fetch_all(pool)
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMethodJson {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub types: Vec<String>,
    pub schema: Option<Value>,
    #[serde(default, alias = "hostFunctions")]
    pub host_functions: bool,
    #[serde(default, alias = "uiHints")]
    pub ui_hints: Vec<String>,
    #[serde(alias = "pluginName")]
    pub plugin_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginTemplateStepJson {
    pub method: String,
    #[serde(default)]
    pub config: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginTemplateJson {
    pub name: String,
    pub title: String,
    pub description: String,
    pub trigger: String,
    #[serde(default)]
    pub steps: Vec<PluginTemplateStepJson>,
    #[serde(default)]
    pub ui_hints: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginLoadMethodJson {
    pub name: String,
    #[serde(default, alias = "hostFunctions")]
    pub host_functions: bool,
}

#[derive(Debug, FromRow)]
pub struct PluginLoadRow {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub wasm_bytes: Vec<u8>,
    pub methods: Value,
}

pub async fn get_for_load(pool: &Pool<Postgres>) -> Result<Vec<PluginLoadRow>, sqlx::Error> {
    sqlx::query_as::<_, PluginLoadRow>(
        r#"
        SELECT
            plugin.id,
            plugin.name,
            plugin.version,
            plugin."wasmBytes" as wasm_bytes,
            (
                SELECT COALESCE(json_agg(agg), '[]'::json)
                FROM (
                    SELECT
                        plugin_method.name,
                        plugin_method."hostFunctions" as "hostFunctions"
                    FROM plugin_method
                    WHERE plugin_method."pluginId" = plugin.id
                ) AS agg
            ) AS methods
        FROM plugin
        WHERE plugin.enabled = true
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn get_by_hash(
    pool: &Pool<Postgres>,
    sha256hash: &[u8],
) -> Result<Option<PluginRow>, sqlx::Error> {
    let query = format!("{PLUGIN_SELECT} WHERE plugin.\"sha256hash\" = $1");
    sqlx::query_as::<_, PluginRow>(&query)
        .bind(sha256hash)
        .fetch_optional(pool)
        .await
}

pub async fn get_by_name(
    pool: &Pool<Postgres>,
    name: &str,
) -> Result<Option<PluginRow>, sqlx::Error> {
    let query = format!("{PLUGIN_SELECT} WHERE plugin.name = $1");
    sqlx::query_as::<_, PluginRow>(&query)
        .bind(name)
        .fetch_optional(pool)
        .await
}

#[derive(Debug, Clone)]
pub struct PluginUpsertInput {
    pub name: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub wasm_bytes: Vec<u8>,
    pub templates: Value,
    pub sha256hash: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PluginMethodUpsertInput {
    pub name: String,
    pub title: String,
    pub description: String,
    pub types: Vec<String>,
    pub host_functions: bool,
    pub schema: Option<Value>,
    pub ui_hints: Vec<String>,
}

pub async fn upsert(
    pool: &Pool<Postgres>,
    input: &PluginUpsertInput,
    methods: &[PluginMethodUpsertInput],
) -> Result<(Uuid, bool), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let existing = get_by_name(pool, &input.name).await?;

    let plugin_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO plugin (
            enabled, name, version, title, description, author,
            "wasmBytes", templates, "sha256hash"
        )
        VALUES (true, $1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (name, version) DO UPDATE SET
            title = EXCLUDED.title,
            description = EXCLUDED.description,
            author = EXCLUDED.author,
            version = EXCLUDED.version,
            "wasmBytes" = EXCLUDED."wasmBytes",
            templates = EXCLUDED.templates,
            "sha256hash" = EXCLUDED."sha256hash",
            "updatedAt" = NOW()
        RETURNING id
        "#,
    )
    .bind(&input.name)
    .bind(&input.version)
    .bind(&input.title)
    .bind(&input.description)
    .bind(&input.author)
    .bind(&input.wasm_bytes)
    .bind(&input.templates)
    .bind(&input.sha256hash)
    .fetch_one(&mut *tx)
    .await?;

    if !methods.is_empty() {
        let method_names: Vec<&str> = methods.iter().map(|method| method.name.as_str()).collect();
        sqlx::query(
            r#"
            DELETE FROM plugin_method
            WHERE "pluginId" = $1
              AND name != ALL($2)
            "#,
        )
        .bind(plugin_id)
        .bind(&method_names)
        .execute(&mut *tx)
        .await?;

        for method in methods {
            sqlx::query(
                r#"
                INSERT INTO plugin_method (
                    "pluginId", name, title, description, types,
                    "hostFunctions", schema, "uiHints"
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT ("pluginId", name) DO UPDATE SET
                    title = EXCLUDED.title,
                    description = EXCLUDED.description,
                    types = EXCLUDED.types,
                    "hostFunctions" = EXCLUDED."hostFunctions",
                    schema = EXCLUDED.schema,
                    "uiHints" = EXCLUDED."uiHints"
                "#,
            )
            .bind(plugin_id)
            .bind(&method.name)
            .bind(&method.title)
            .bind(&method.description)
            .bind(&method.types)
            .bind(method.host_functions)
            .bind(&method.schema)
            .bind(&method.ui_hints)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok((plugin_id, existing.is_some()))
}
