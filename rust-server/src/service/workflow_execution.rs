use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::plugin;
use crate::models::db::sessions::AuthSession;
use crate::models::db::users::AuthUserDb;
use crate::models::db::workflow::{self, WorkflowRunRow, WorkflowRunStep};
use crate::models::dto::auth::AuthDto;
use crate::service::asset::{AssetService, UpdateAssetReq};
use crate::service::job::JobService;
use crate::service::plugin_runtime::{self, PluginRuntime};
use crate::service::websocket::WebSocketHub;
use crate::utils::workflow::TYPE_ASSET_V1;

const WORKFLOW_TYPE_ASSET_V1: &str = TYPE_ASSET_V1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowExecutionOutcome {
    Success,
    Failed,
    Skipped,
}

pub struct WorkflowExecutionService {
    pool: PgPool,
    runtime: Arc<PluginRuntime>,
    asset_service: AssetService,
}

impl WorkflowExecutionService {
    pub fn new(
        pool: PgPool,
        runtime: Arc<PluginRuntime>,
        jobs: JobService,
        websocket: WebSocketHub,
    ) -> Self {
        Self {
            pool: pool.clone(),
            runtime,
            asset_service: AssetService::new(pool, jobs, websocket),
        }
    }

    pub async fn load_plugins(&self) -> Result<(), String> {
        let rows = plugin::get_for_load(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        self.runtime.load_all(rows);
        Ok(())
    }

    pub async fn execute(
        &self,
        workflow_id: &Uuid,
        asset_id: &Uuid,
    ) -> Result<WorkflowExecutionOutcome, String> {
        let Some(workflow) = workflow::get_for_workflow_run(&self.pool, workflow_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(WorkflowExecutionOutcome::Skipped);
        };

        let steps: Vec<WorkflowRunStep> =
            serde_json::from_value(workflow.steps.clone()).unwrap_or_default();
        let Some(workflow_type) = infer_workflow_type(&steps) else {
            return Err("Unable to infer workflow event type from steps".to_string());
        };

        match workflow_type {
            WORKFLOW_TYPE_ASSET_V1 => {
                self.execute_asset_v1(&workflow, &steps, asset_id).await
            }
            _ => Ok(WorkflowExecutionOutcome::Skipped),
        }
    }

    async fn execute_asset_v1(
        &self,
        workflow: &WorkflowRunRow,
        steps: &[WorkflowRunStep],
        asset_id: &Uuid,
    ) -> Result<WorkflowExecutionOutcome, String> {
        let mut asset_data = self.read_asset_v1(asset_id).await?;
        let owner_id = parse_owner_id(&asset_data)?;

        for step in steps {
            if step.method_name.starts_with("noop") {
                continue;
            }

            let payload = json!({
                "trigger": workflow.trigger,
                "type": WORKFLOW_TYPE_ASSET_V1,
                "config": step.config.clone().unwrap_or_else(|| json!({})),
                "workflow": {
                    "id": workflow.id,
                    "authToken": self.runtime.sign_auth_token(&owner_id)?,
                    "stepId": step.id,
                },
                "data": {
                    "asset": asset_data,
                },
            });

            let plugin_key = plugin_runtime::plugin_key(&step.plugin_id, step.host_functions);
            let result = self
                .runtime
                .call_method(&plugin_key, &step.method_name, &payload)?;

            if result.get("changes").is_some() {
                apply_asset_v1_changes(&self.asset_service, &owner_id, asset_id, &result).await?;
                asset_data = self.read_asset_v1(asset_id).await?;
            }

            if let Some(config) = result.get("config") {
                workflow::update_step_config(&self.pool, &step.id, config)
                    .await
                    .map_err(|err| err.to_string())?;
            }

            let should_continue = result
                .get("workflow")
                .and_then(|value| value.get("continue"))
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            if !should_continue {
                break;
            }
        }

        Ok(WorkflowExecutionOutcome::Success)
    }

    async fn read_asset_v1(&self, asset_id: &Uuid) -> Result<Value, String> {
        workflow::get_for_asset_v1(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "Asset not found".to_string())
    }
}

fn infer_workflow_type(steps: &[WorkflowRunStep]) -> Option<&'static str> {
    if steps.is_empty() {
        return None;
    }

    for target_type in [WORKFLOW_TYPE_ASSET_V1] {
        let missing = steps
            .iter()
            .any(|step| !step.types.iter().any(|value| value == target_type));
        if !missing {
            return Some(target_type);
        }
    }

    None
}

fn parse_owner_id(asset: &Value) -> Result<Uuid, String> {
    asset
        .get("ownerId")
        .and_then(|value| value.as_str())
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| "Missing ownerId".to_string())
}

fn workflow_auth(user_id: &Uuid) -> AuthDto {
    AuthDto {
        user: AuthUserDb {
            id: *user_id,
            is_admin: false,
            name: String::new(),
            email: String::new(),
            quota_usage_in_bytes: 0,
            quota_size_in_bytes: None,
        },
        api_key: None,
        session: Some(AuthSession {
            id: "workflow".to_string(),
            has_elevated_permission: true,
        }),
        shared_link: None,
    }
}

async fn apply_asset_v1_changes(
    asset_service: &AssetService,
    auth_user_id: &Uuid,
    asset_id: &Uuid,
    result: &Value,
) -> Result<(), String> {
    let Some(asset) = result.get("changes").and_then(|value| value.get("asset")) else {
        return Ok(());
    };

    let exif = asset.get("exifInfo");
    let dto = UpdateAssetReq {
        is_favorite: asset.get("isFavorite").and_then(|value| value.as_bool()),
        visibility: asset
            .get("visibility")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        date_time_original: exif
            .and_then(|value| value.get("dateTimeOriginal"))
            .and_then(|value| value.as_str())
            .map(str::to_string),
        latitude: exif.and_then(|value| value.get("latitude")).and_then(|v| v.as_f64()),
        longitude: exif
            .and_then(|value| value.get("longitude"))
            .and_then(|v| v.as_f64()),
        rating: exif.and_then(|value| value.get("rating")).and_then(parse_rating),
        description: exif
            .and_then(|value| value.get("description"))
            .and_then(|value| value.as_str())
            .map(str::to_string),
        live_photo_video_id: None,
    };

    asset_service
        .update(&workflow_auth(auth_user_id), asset_id, &dto)
        .await
        .map_err(|err| err.to_string())?;

    Ok(())
}

fn parse_rating(value: &Value) -> Option<i32> {
    if value.is_null() {
        return None;
    }
    value.as_i64().map(|value| value as i32)
}
