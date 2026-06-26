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
pub struct PluginTemplateStepJson {
    pub method: String,
    #[serde(default)]
    pub config: Option<Value>,
    pub enabled: Option<bool>,
}
