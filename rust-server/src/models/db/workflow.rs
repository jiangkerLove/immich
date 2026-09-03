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
    pub logging: bool,
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
        workflow.logging,
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
    logging: Option<bool>,
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
        bind_idx += 1;
    }
    if logging.is_some() {
        query.push_str(&format!(" AND workflow.logging = ${bind_idx}"));
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
    if let Some(logging) = logging {
        q = q.bind(logging);
    }
    q.fetch_all(pool).await
}

pub async fn get_by_id(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<WorkflowRow>, sqlx::Error> {
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
    logging: bool,
    steps: &[(Uuid, bool, Option<Value>)],
) -> Result<WorkflowRow, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO workflow ("ownerId", trigger, name, description, enabled, logging)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(owner_id)
    .bind(trigger)
    .bind(name)
    .bind(description)
    .bind(enabled)
    .bind(logging)
    .fetch_one(&mut *tx)
    .await?;

    replace_steps(&mut tx, &id, steps).await?;
    tx.commit().await?;

    get_by_id(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn update(
    pool: &Pool<Postgres>,
    id: &Uuid,
    trigger: Option<&str>,
    name: Option<Option<&str>>,
    description: Option<Option<&str>>,
    enabled: Option<bool>,
    logging: Option<bool>,
    steps: Option<&[(Uuid, bool, Option<Value>)]>,
) -> Result<WorkflowRow, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let current = get_by_id(pool, id).await?.ok_or(sqlx::Error::RowNotFound)?;

    if logging == Some(false) {
        sqlx::query(r#"DELETE FROM workflow_log WHERE "workflowId" = $1"#)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    if trigger.is_some()
        || name.is_some()
        || description.is_some()
        || enabled.is_some()
        || logging.is_some()
    {
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
                logging = $6,
                "updatedAt" = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(trigger.unwrap_or(&current.trigger))
        .bind(next_name)
        .bind(next_description)
        .bind(enabled.unwrap_or(current.enabled))
        .bind(logging.unwrap_or(current.logging))
        .execute(&mut *tx)
        .await?;
    }

    if let Some(steps) = steps {
        replace_steps(&mut tx, id, steps).await?;
    }

    tx.commit().await?;

    get_by_id(pool, id).await?.ok_or(sqlx::Error::RowNotFound)
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

#[derive(Debug, FromRow)]
pub struct WorkflowRunRow {
    pub id: Uuid,
    pub name: Option<String>,
    pub trigger: String,
    pub logging: bool,
    pub steps: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunStep {
    pub id: Uuid,
    pub config: Option<Value>,
    pub plugin_id: Uuid,
    pub method_name: String,
    pub types: Vec<String>,
    pub host_functions: bool,
}

pub async fn get_for_workflow_run(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<WorkflowRunRow>, sqlx::Error> {
    sqlx::query_as::<_, WorkflowRunRow>(
        r#"
        SELECT
            workflow.id,
            workflow.name,
            workflow.trigger,
            workflow.logging,
            (
                SELECT COALESCE(json_agg(step ORDER BY workflow_step."order"), '[]'::json)
                FROM (
                    SELECT
                        workflow_step.id,
                        workflow_step.config,
                        plugin_method."pluginId" as "pluginId",
                        plugin_method.name as "methodName",
                        plugin_method.types,
                        plugin_method."hostFunctions" as "hostFunctions"
                    FROM workflow_step
                    INNER JOIN plugin_method ON plugin_method.id = workflow_step."pluginMethodId"
                    WHERE workflow_step."workflowId" = workflow.id
                      AND workflow_step.enabled = true
                    ORDER BY workflow_step."order"
                ) AS step
            ) AS steps
        FROM workflow
        WHERE workflow.id = $1
          AND workflow.enabled = true
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn get_for_asset_v1(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT json_build_object(
            'id', a.id,
            'ownerId', a."ownerId",
            'stackId', a."stackId",
            'livePhotoVideoId', a."livePhotoVideoId",
            'libraryId', a."libraryId",
            'duplicateId', a."duplicateId",
            'createdAt', a."createdAt",
            'updatedAt', a."updatedAt",
            'deletedAt', a."deletedAt",
            'fileCreatedAt', a."fileCreatedAt",
            'fileModifiedAt', a."fileModifiedAt",
            'localDateTime', a."localDateTime",
            'type', a.type,
            'status', a.status,
            'visibility', a.visibility,
            'duration', a.duration,
            'checksum', encode(a.checksum, 'base64'),
            'originalPath', a."originalPath",
            'originalFileName', a."originalFileName",
            'isOffline', a."isOffline",
            'isFavorite', a."isFavorite",
            'isExternal', a."isExternal",
            'isEdited', a."isEdited",
            'tags', COALESCE((
                SELECT json_agg(json_build_object(
                    'id', t.id,
                    'value', t.value,
                    'createdAt', t."createdAt",
                    'updatedAt', t."updatedAt",
                    'color', t.color,
                    'parentId', t."parentId"
                ))
                FROM tag t
                INNER JOIN tag_asset ta ON ta."tagId" = t.id
                WHERE ta."assetId" = a.id
            ), '[]'::json),
            'exifInfo', (
                SELECT json_build_object(
                    'make', e.make,
                    'model', e.model,
                    'orientation', e.orientation,
                    'dateTimeOriginal', e."dateTimeOriginal",
                    'modifyDate', e."modifyDate",
                    'exifImageWidth', e."exifImageWidth",
                    'exifImageHeight', e."exifImageHeight",
                    'fileSizeInByte', e."fileSizeInByte",
                    'lensModel', e."lensModel",
                    'fNumber', e."fNumber",
                    'focalLength', e."focalLength",
                    'iso', e.iso,
                    'latitude', e.latitude,
                    'longitude', e.longitude,
                    'city', e.city,
                    'state', e.state,
                    'country', e.country,
                    'description', e.description,
                    'fps', e.fps,
                    'exposureTime', e."exposureTime",
                    'livePhotoCID', e."livePhotoCID",
                    'timeZone', e."timeZone",
                    'projectionType', e."projectionType",
                    'profileDescription', e."profileDescription",
                    'colorspace', e.colorspace,
                    'bitsPerSample', e."bitsPerSample",
                    'autoStackId', e."autoStackId",
                    'rating', e.rating,
                    'tags', e.tags,
                    'updatedAt', e."updatedAt"
                )
                FROM asset_exif e
                WHERE e."assetId" = a.id
            )
        )
        FROM asset a
        WHERE a.id = $1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn update_step_config(
    pool: &Pool<Postgres>,
    step_id: &Uuid,
    config: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE workflow_step
        SET config = $2
        WHERE id = $1
        "#,
    )
    .bind(step_id)
    .bind(config)
    .execute(pool)
    .await?;
    Ok(())
}
