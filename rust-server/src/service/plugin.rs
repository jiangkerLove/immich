use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::auth_permission::Permission;
use crate::models::db::plugin::{
    self, PluginMethodJson, PluginMethodRow, PluginRow, PluginTemplateJson,
};
use crate::models::response::response::ErrorResp;
use crate::utils::permission::require_permission;
use crate::utils::workflow::{as_plugin_key, is_method_compatible};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSearchQuery {
    pub id: Option<Uuid>,
    pub enabled: Option<bool>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMethodSearchQuery {
    pub id: Option<Uuid>,
    pub enabled: Option<bool>,
    pub name: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub workflow_type: Option<String>,
    pub trigger: Option<String>,
    pub plugin_name: Option<String>,
    pub plugin_version: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMethodResponse {
    pub key: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub types: Vec<String>,
    pub ui_hints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    pub host_functions: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginResponse {
    pub id: Uuid,
    pub name: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub created_at: String,
    pub updated_at: String,
    pub methods: Vec<PluginMethodResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginTemplateStepResponse {
    pub method: String,
    pub config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginTemplateResponse {
    pub key: String,
    pub title: String,
    pub description: String,
    pub trigger: String,
    pub steps: Vec<PluginTemplateStepResponse>,
    pub ui_hints: Vec<String>,
}

#[derive(Clone)]
pub struct PluginService {
    pool: PgPool,
}

impl PluginService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn search(
        &self,
        auth: &crate::models::dto::auth::AuthDto,
        query: &PluginSearchQuery,
    ) -> Result<Vec<PluginResponse>, ErrorResp> {
        require_permission(auth, Permission::PluginRead)?;
        let rows = plugin::search(
            &self.pool,
            query.id,
            query.name.as_deref(),
            query.title.as_deref(),
            query.description.as_deref(),
            query.version.as_deref(),
        )
        .await
        .map_err(ErrorResp::from)?;
        Ok(rows.into_iter().map(map_plugin).collect())
    }

    pub async fn get(
        &self,
        auth: &crate::models::dto::auth::AuthDto,
        id: &Uuid,
    ) -> Result<PluginResponse, ErrorResp> {
        require_permission(auth, Permission::PluginRead)?;
        let row = plugin::get_by_id(&self.pool, id)
            .await
            .map_err(ErrorResp::from)?
            .ok_or_else(|| ErrorResp::BadRequest("Plugin not found".to_string()))?;
        Ok(map_plugin(row))
    }

    pub async fn search_methods(
        &self,
        auth: &crate::models::dto::auth::AuthDto,
        query: &PluginMethodSearchQuery,
    ) -> Result<Vec<PluginMethodResponse>, ErrorResp> {
        require_permission(auth, Permission::PluginRead)?;
        let rows = plugin::search_methods(
            &self.pool,
            query.id,
            query.name.as_deref(),
            query.title.as_deref(),
            query.description.as_deref(),
            query.workflow_type.as_deref(),
            query.plugin_name.as_deref(),
            query.plugin_version.as_deref(),
        )
        .await
        .map_err(ErrorResp::from)?;

        Ok(rows
            .into_iter()
            .filter(|row| {
                query
                    .trigger
                    .as_deref()
                    .map(|trigger| is_method_compatible(&row.types, trigger))
                    .unwrap_or(true)
            })
            .map(map_method_row)
            .collect())
    }

    pub async fn search_templates(
        &self,
        auth: &crate::models::dto::auth::AuthDto,
    ) -> Result<Vec<PluginTemplateResponse>, ErrorResp> {
        require_permission(auth, Permission::PluginRead)?;
        let rows = plugin::search(&self.pool, None, None, None, None, None)
            .await
            .map_err(ErrorResp::from)?;

        let mut templates = Vec::new();
        for row in rows {
            let plugin_name = row.name.clone();
            let parsed: Vec<PluginTemplateJson> =
                serde_json::from_value(row.templates).unwrap_or_default();
            for template in parsed {
                templates.push(map_template(&plugin_name, template));
            }
        }
        Ok(templates)
    }
}

fn map_plugin(row: PluginRow) -> PluginResponse {
    let methods: Vec<PluginMethodJson> = serde_json::from_value(row.methods).unwrap_or_default();
    PluginResponse {
        id: row.id,
        name: row.name,
        title: row.title,
        description: row.description,
        author: row.author,
        version: row.version,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
        methods: methods.into_iter().map(map_method_json).collect(),
    }
}

fn map_method_json(method: PluginMethodJson) -> PluginMethodResponse {
    PluginMethodResponse {
        key: as_plugin_key(&method.plugin_name, &method.name),
        name: method.name,
        title: method.title,
        description: method.description,
        types: method.types,
        ui_hints: method.ui_hints,
        schema: method.schema,
        host_functions: method.host_functions,
    }
}

fn map_method_row(row: PluginMethodRow) -> PluginMethodResponse {
    PluginMethodResponse {
        key: as_plugin_key(&row.plugin_name, &row.name),
        name: row.name,
        title: row.title,
        description: row.description,
        types: row.types,
        ui_hints: row.ui_hints,
        schema: row.schema,
        host_functions: row.host_functions,
    }
}

fn map_template(plugin_name: &str, template: PluginTemplateJson) -> PluginTemplateResponse {
    PluginTemplateResponse {
        key: as_plugin_key(plugin_name, &template.name),
        title: template.title,
        description: template.description,
        trigger: template.trigger,
        steps: template
            .steps
            .into_iter()
            .map(|step| PluginTemplateStepResponse {
                method: step.method,
                config: step.config,
                enabled: step.enabled,
            })
            .collect(),
        ui_hints: template.ui_hints,
    }
}

pub fn find_validation_method<'a>(
    methods: &'a [plugin::PluginMethodValidationRow],
    method: &str,
) -> Option<&'a plugin::PluginMethodValidationRow> {
    crate::utils::workflow::parse_method_string(method).and_then(|parsed| {
        methods
            .iter()
            .find(|m| m.plugin_name == parsed.plugin_name && m.name == parsed.method_name)
    })
}
