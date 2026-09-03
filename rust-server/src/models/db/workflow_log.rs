use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct WorkflowLogRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub result: String,
    pub workflow_id: Uuid,
    pub workflow_step_id: Option<Uuid>,
    pub trigger_data_id: Option<Uuid>,
    pub step_order: Option<i32>,
    pub plugin_id: Option<Uuid>,
    pub method_name: Option<String>,
}

pub async fn get_logs(
    pool: &Pool<Postgres>,
    workflow_id: &Uuid,
    result: Option<&str>,
    before: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Vec<WorkflowLogRow>, sqlx::Error> {
    let mut query = sqlx::QueryBuilder::new(
        r#"
        SELECT
            workflow_log.id,
            workflow_log."createdAt" as created_at,
            workflow_log.result,
            workflow_log."workflowId" as workflow_id,
            workflow_log."workflowStepId" as workflow_step_id,
            workflow_log."triggerDataId" as trigger_data_id,
            workflow_step."order" as step_order,
            plugin.id as plugin_id,
            plugin_method.name as method_name
        FROM workflow_log
        LEFT JOIN workflow_step ON workflow_step.id = workflow_log."workflowStepId"
        LEFT JOIN plugin_method ON plugin_method.id = workflow_step."pluginMethodId"
        LEFT JOIN plugin ON plugin.id = plugin_method."pluginId"
        WHERE workflow_log."workflowId" =
        "#,
    );
    query.push_bind(workflow_id);

    if let Some(result) = result {
        query.push(r#" AND workflow_log.result = "#);
        query.push_bind(result);
    }
    if let Some(before) = before {
        query.push(r#" AND workflow_log."createdAt" < "#);
        query.push_bind(before);
    }

    query.push(r#" ORDER BY workflow_log."createdAt" DESC LIMIT "#);
    query.push_bind(limit);

    query.build_query_as().fetch_all(pool).await
}

pub async fn insert_log(
    pool: &Pool<Postgres>,
    workflow_id: &Uuid,
    result: &str,
    workflow_step_id: Option<&Uuid>,
    trigger_data_id: Option<&Uuid>,
    run_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO workflow_log ("workflowId", result, "workflowStepId", "triggerDataId", "runId")
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(workflow_id)
    .bind(result)
    .bind(workflow_step_id)
    .bind(trigger_data_id)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}
