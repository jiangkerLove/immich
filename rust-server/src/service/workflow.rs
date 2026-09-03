use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::auth_permission::Permission;
use crate::models::db::plugin;
use crate::models::db::workflow::{self, WorkflowRow, WorkflowStepJson};
use crate::models::db::workflow_log;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::plugin::find_validation_method;
use crate::utils::permission::require_permission;
use crate::utils::workflow::{as_plugin_key, get_workflow_triggers, is_method_compatible, WorkflowTriggerResponse};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSearchQuery {
    pub id: Option<Uuid>,
    pub trigger: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepReq {
    pub method: String,
    pub config: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCreateReq {
    pub trigger: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub logging: Option<bool>,
    pub steps: Option<Vec<WorkflowStepReq>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowUpdateReq {
    pub trigger: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub logging: Option<bool>,
    pub steps: Option<Vec<WorkflowStepReq>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepResponse {
    pub method: String,
    pub config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowResponse {
    pub id: Uuid,
    pub trigger: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub enabled: bool,
    pub logging: bool,
    pub steps: Vec<WorkflowStepResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowShareResponse {
    pub trigger: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub steps: Vec<WorkflowStepResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowGetLogsQuery {
    pub result: Option<String>,
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowLogStepResponse {
    pub method: String,
    pub index: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowLogEntryResponse {
    pub id: Uuid,
    pub at: String,
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_data_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_step: Option<WorkflowLogStepResponse>,
}

#[derive(Clone)]
pub struct WorkflowService {
    pool: PgPool,
}

impl WorkflowService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn get_triggers(&self) -> Vec<WorkflowTriggerResponse> {
        get_workflow_triggers()
    }

    pub async fn search(
        &self,
        auth: &AuthDto,
        query: &WorkflowSearchQuery,
    ) -> Result<Vec<WorkflowResponse>, ErrorResp> {
        require_permission(auth, Permission::WorkflowRead)?;
        let rows = workflow::search(
            &self.pool,
            &auth.user.id,
            query.id,
            query.trigger.as_deref(),
            query.enabled,
        )
        .await
        .map_err(ErrorResp::from)?;
        Ok(rows.into_iter().map(map_workflow).collect())
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<WorkflowResponse, ErrorResp> {
        self.require_workflow_access(auth, id, Permission::WorkflowRead)
            .await?;
        let row = self.find_or_fail(id).await?;
        Ok(map_workflow(row))
    }

    pub async fn share(&self, auth: &AuthDto, id: &Uuid) -> Result<WorkflowShareResponse, ErrorResp> {
        self.require_workflow_access(auth, id, Permission::WorkflowRead)
            .await?;
        let row = self.find_or_fail(id).await?;
        Ok(map_workflow_share(row))
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &WorkflowCreateReq,
    ) -> Result<WorkflowResponse, ErrorResp> {
        require_permission(auth, Permission::WorkflowCreate)?;
        let steps = self
            .resolve_steps(dto.steps.as_deref().unwrap_or(&[]), &dto.trigger)
            .await?;
        let row = workflow::create(
            &self.pool,
            &auth.user.id,
            &dto.trigger,
            dto.name.as_deref(),
            dto.description.as_deref(),
            dto.enabled.unwrap_or(true),
            dto.logging.unwrap_or(false),
            &steps,
        )
        .await
        .map_err(ErrorResp::from)?;
        Ok(map_workflow(row))
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &WorkflowUpdateReq,
    ) -> Result<WorkflowResponse, ErrorResp> {
        self.require_workflow_access(auth, id, Permission::WorkflowUpdate)
            .await?;
        let current = self.find_or_fail(id).await?;
        let trigger = dto.trigger.as_deref().unwrap_or(&current.trigger);
        let steps = if let Some(steps) = &dto.steps {
            Some(self.resolve_steps(steps, trigger).await?)
        } else {
            None
        };

        let row = workflow::update(
            &self.pool,
            id,
            dto.trigger.as_deref(),
            dto.name.as_ref().map(|name| Some(name.as_str())),
            dto.description.as_ref().map(|desc| Some(desc.as_str())),
            dto.enabled,
            dto.logging,
            steps.as_deref(),
        )
        .await
        .map_err(ErrorResp::from)?;
        Ok(map_workflow(row))
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        self.require_workflow_access(auth, id, Permission::WorkflowDelete)
            .await?;
        workflow::delete(&self.pool, id)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn get_logs(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        query: &WorkflowGetLogsQuery,
    ) -> Result<Vec<WorkflowLogEntryResponse>, ErrorResp> {
        self.require_workflow_access(auth, id, Permission::WorkflowLogs)
            .await?;
        let limit = query.limit.unwrap_or(50).clamp(1, 250);
        let rows = workflow_log::get_logs(
            &self.pool,
            id,
            query.result.as_deref(),
            query.before,
            limit,
        )
        .await
        .map_err(ErrorResp::from)?;
        Ok(rows.into_iter().map(map_workflow_log).collect())
    }

    async fn find_or_fail(&self, id: &Uuid) -> Result<WorkflowRow, ErrorResp> {
        workflow::get_by_id(&self.pool, id)
            .await
            .map_err(ErrorResp::from)?
            .ok_or_else(|| ErrorResp::BadRequest("Workflow not found".to_string()))
    }

    async fn require_workflow_access(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        permission: Permission,
    ) -> Result<(), ErrorResp> {
        require_permission(auth, permission)?;
        if auth.user.is_admin {
            return Ok(());
        }
        let row = self.find_or_fail(id).await?;
        if row.owner_id != auth.user.id {
            return Err(ErrorResp::Forbidden("Forbidden".to_string()));
        }
        Ok(())
    }

    async fn resolve_steps(
        &self,
        steps: &[WorkflowStepReq],
        trigger: &str,
    ) -> Result<Vec<(Uuid, bool, Option<Value>)>, ErrorResp> {
        let methods = plugin::get_for_validation(&self.pool)
            .await
            .map_err(ErrorResp::from)?;
        let mut resolved = Vec::with_capacity(steps.len());

        for step in steps {
            let plugin_method = find_validation_method(&methods, &step.method).ok_or_else(|| {
                ErrorResp::BadRequest(format!("Unknown method {}", step.method))
            })?;
            if !is_method_compatible(&plugin_method.types, trigger) {
                return Err(ErrorResp::BadRequest(format!(
                    "Method \"{}\" is incompatible with workflow trigger: \"{trigger}\"",
                    step.method
                )));
            }
            resolved.push((
                plugin_method.id,
                step.enabled.unwrap_or(true),
                step.config.clone(),
            ));
        }

        Ok(resolved)
    }
}

fn map_workflow(row: WorkflowRow) -> WorkflowResponse {
    WorkflowResponse {
        id: row.id,
        trigger: row.trigger,
        name: row.name,
        description: row.description,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
        enabled: row.enabled,
        logging: row.logging,
        steps: map_steps(row.steps, false),
    }
}

fn map_workflow_share(row: WorkflowRow) -> WorkflowShareResponse {
    WorkflowShareResponse {
        trigger: row.trigger,
        name: row.name,
        description: row.description,
        steps: map_steps(row.steps, true),
    }
}

fn map_steps(steps: Value, for_share: bool) -> Vec<WorkflowStepResponse> {
    let parsed: Vec<WorkflowStepJson> = serde_json::from_value(steps).unwrap_or_default();
    parsed
        .into_iter()
        .map(|step| {
            let enabled = if for_share && step.enabled {
                None
            } else if for_share {
                Some(false)
            } else {
                Some(step.enabled)
            };
            WorkflowStepResponse {
                method: as_plugin_key(&step.plugin_name, &step.method_name),
                config: step.config,
                enabled,
            }
        })
        .collect()
}

fn map_workflow_log(row: workflow_log::WorkflowLogRow) -> WorkflowLogEntryResponse {
    let last_step = match (row.step_order, row.plugin_id, row.method_name) {
        (Some(index), Some(plugin_id), Some(method_name)) => Some(WorkflowLogStepResponse {
            method: format!("{plugin_id}#{method_name}"),
            index,
        }),
        _ => None,
    };

    WorkflowLogEntryResponse {
        id: row.id,
        at: row.created_at.to_rfc3339(),
        result: row.result,
        trigger_data_id: row.trigger_data_id,
        last_step,
    }
}
