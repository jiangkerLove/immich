use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct WorkflowRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    pub trigger: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub steps: Value,
}

const WORKFLOW_SELECT: &str = r#"
    SELECT
        workflow.id,
        workflow."ownerId" as owner_id,
        workflow.name,
        workflow.description,
        workflow.trigger,
        workflow.enabled,
        workflow."createdAt" as created_at,
        workflow."updatedAt" as updated_at,
        (
            SELECT COALESCE(json_agg(agg), '[]'::json)
            FROM (
                SELECT
                    plugin.name as "pluginName",
                    plugin_method.name as "methodName",
                    workflow_step.config,
                    workflow_step.enabled
                FROM workflow_step
                INNER JOIN plugin_method ON plugin_method.id = workflow_step."pluginMethodId"
                INNER JOIN plugin ON plugin.id = plugin_method."pluginId"
                WHERE workflow_step."workflowId" = workflow.id
                ORDER BY workflow_step."order"
            ) AS agg
        ) AS steps
    FROM workflow
"#;

pub async fn search(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    id: Option<Uuid>,
    trigger: Option<&str>,
    enabled: Option<bool>,
) -> Result<Vec<WorkflowRow>, sqlx::Error> {
    let mut query = String::from(WORKFLOW_SELECT);
    query.push_str(r#" WHERE workflow."ownerId" = $1"#);
    let mut bind_idx = 2u8;

    if id.is_some() {
        query.push_str(&format!(" AND workflow.id = ${bind_idx}"));
        bind_idx += 1;
    }
    if trigger.is_some() {
        query.push_str(&format!(" AND workflow.trigger = ${bind_idx}"));
        bind_idx += 1;
    }
    if enabled.is_some() {
        query.push_str(&format!(" AND workflow.enabled = ${bind_idx}"));
        let _ = bind_idx;
    }
    query.push_str(r#" ORDER BY workflow."createdAt" DESC"#);

    let mut q = sqlx::query_as::<_, WorkflowRow>(&query).bind(user_id);
    if let Some(id) = id {
        q = q.bind(id);
    }
    if let Some(trigger) = trigger {
        q = q.bind(trigger);
    }
    if let Some(enabled) = enabled {
        q = q.bind(enabled);
    }
    q.fetch_all(pool).await
}

pub async fn get_by_id(pool: &Pool<Postgres>, id: &Uuid) -> Result<Option<WorkflowRow>, sqlx::Error> {
    let query = format!("{WORKFLOW_SELECT} WHERE workflow.id = $1");
    sqlx::query_as::<_, WorkflowRow>(&query)
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn create(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    trigger: &str,
    name: Option<&str>,
    description: Option<&str>,
    enabled: bool,
    steps: &[(Uuid, bool, Option<Value>)],
) -> Result<WorkflowRow, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO workflow ("ownerId", trigger, name, description, enabled)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(owner_id)
    .bind(trigger)
    .bind(name)
    .bind(description)
    .bind(enabled)
    .fetch_one(&mut *tx)
    .await?;

    replace_steps(&mut tx, &id, steps).await?;
    tx.commit().await?;

    get_by_id(pool, &id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn update(
    pool: &Pool<Postgres>,
    id: &Uuid,
    trigger: Option<&str>,
    name: Option<Option<&str>>,
    description: Option<Option<&str>>,
    enabled: Option<bool>,
    steps: Option<&[(Uuid, bool, Option<Value>)]>,
) -> Result<WorkflowRow, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let current = get_by_id(pool, id).await?.ok_or(sqlx::Error::RowNotFound)?;

    if trigger.is_some() || name.is_some() || description.is_some() || enabled.is_some() {
        let next_name = match name {
            None => current.name.as_deref(),
            Some(value) => value,
        };
        let next_description = match description {
            None => current.description.as_deref(),
            Some(value) => value,
        };
        sqlx::query(
            r#"
            UPDATE workflow
            SET
                trigger = $2,
                name = $3,
                description = $4,
                enabled = $5,
                "updatedAt" = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(trigger.unwrap_or(&current.trigger))
        .bind(next_name)
        .bind(next_description)
        .bind(enabled.unwrap_or(current.enabled))
        .execute(&mut *tx)
        .await?;
    }

    if let Some(steps) = steps {
        replace_steps(&mut tx, id, steps).await?;
    }

    tx.commit().await?;

    get_by_id(pool, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn delete(pool: &Pool<Postgres>, id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM workflow WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn replace_steps(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    workflow_id: &Uuid,
    steps: &[(Uuid, bool, Option<Value>)],
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM workflow_step WHERE "workflowId" = $1"#)
        .bind(workflow_id)
        .execute(&mut **tx)
        .await?;

    for (order, (plugin_method_id, enabled, config)) in steps.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO workflow_step ("workflowId", "pluginMethodId", enabled, config, "order")
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(workflow_id)
        .bind(plugin_method_id)
        .bind(enabled)
        .bind(config)
        .bind(order as i32)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepJson {
    #[serde(alias = "pluginName")]
    pub plugin_name: String,
    #[serde(alias = "methodName")]
    pub method_name: String,
    pub config: Option<Value>,
    pub enabled: bool,
}
