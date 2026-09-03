pub const TRIGGER_ASSET_CREATE: &str = "AssetCreate";
pub const TRIGGER_ASSET_METADATA: &str = "AssetMetadataExtraction";
pub const TYPE_ASSET_V1: &str = "AssetV1";

pub const WORKFLOW_RESULT_COMPLETED: &str = "completed";
pub const WORKFLOW_RESULT_HALTED: &str = "halted";
pub const WORKFLOW_RESULT_ERROR: &str = "error";

pub fn get_workflow_triggers() -> Vec<WorkflowTriggerResponse> {
    vec![
        WorkflowTriggerResponse {
            trigger: TRIGGER_ASSET_CREATE.to_string(),
            types: vec![TYPE_ASSET_V1.to_string()],
        },
        WorkflowTriggerResponse {
            trigger: TRIGGER_ASSET_METADATA.to_string(),
            types: vec![TYPE_ASSET_V1.to_string()],
        },
    ]
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTriggerResponse {
    pub trigger: String,
    pub types: Vec<String>,
}

pub fn is_method_compatible(method_types: &[String], trigger: &str) -> bool {
    let valid_types = match trigger {
        TRIGGER_ASSET_CREATE | TRIGGER_ASSET_METADATA => &[TYPE_ASSET_V1][..],
        _ => return false,
    };

    method_types
        .iter()
        .any(|method_type| valid_types.contains(&method_type.as_str()))
}

pub fn as_plugin_key(plugin_name: &str, method_name: &str) -> String {
    format!("{plugin_name}#{method_name}")
}

#[derive(Debug, Clone)]
pub struct ParsedMethod {
    pub plugin_name: String,
    pub method_name: String,
}

pub fn parse_method_string(method: &str) -> Option<ParsedMethod> {
    let (plugin_part, method_name) = method.rsplit_once('#')?;
    if plugin_part.is_empty() || method_name.is_empty() {
        return None;
    }

    let plugin_name = plugin_part.split('@').next()?.to_string();
    if plugin_name.is_empty() || method_name.contains('@') || method_name.contains('#') {
        return None;
    }

    Some(ParsedMethod {
        plugin_name,
        method_name: method_name.to_string(),
    })
}
