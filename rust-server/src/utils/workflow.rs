pub const TRIGGER_ASSET_CREATE: &str = "AssetCreate";
pub const TRIGGER_ASSET_METADATA: &str = "AssetMetadataExtraction";
pub const TRIGGER_ASSET_TAGGED: &str = "AssetTagged";
pub const TYPE_ASSET_V1: &str = "AssetV1";

pub const WORKFLOW_RESULT_COMPLETED: &str = "completed";
pub const WORKFLOW_RESULT_HALTED: &str = "halted";
pub const WORKFLOW_RESULT_ERROR: &str = "error";

/// How a workflow run ended, used to decide what to persist in `workflow_log`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowRunEnd {
    Completed,
    Halted,
    Error,
}

/// Fields written to `workflow_log` for a given run outcome.
/// Matches the TypeScript `WorkflowExecutionService` insert shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkflowRunLog {
    pub result: &'static str,
    pub include_step_id: bool,
}

pub fn should_write_workflow_log(logging: bool) -> bool {
    logging
}

pub fn workflow_run_log(end: WorkflowRunEnd) -> WorkflowRunLog {
    match end {
        WorkflowRunEnd::Completed => WorkflowRunLog {
            result: WORKFLOW_RESULT_COMPLETED,
            include_step_id: false,
        },
        WorkflowRunEnd::Halted => WorkflowRunLog {
            result: WORKFLOW_RESULT_HALTED,
            include_step_id: true,
        },
        WorkflowRunEnd::Error => WorkflowRunLog {
            result: WORKFLOW_RESULT_ERROR,
            include_step_id: true,
        },
    }
}

/// Plugin methods halt the remaining steps by returning `{ workflow: { continue: false } }`.
pub fn plugin_result_should_continue(result: &serde_json::Value) -> bool {
    result
        .get("workflow")
        .and_then(|value| value.get("continue"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
}

/// Matches TypeScript `new RegExp(pattern.replaceAll('.', '\\.').replaceAll('*', '.*'))`.
pub fn hostname_matches_allowed_hosts(hostname: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        let escaped = pattern.replace('.', r"\.").replace('*', ".*");
        regex::Regex::new(&escaped)
            .map(|re| re.is_match(hostname))
            .unwrap_or(false)
    })
}

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
        WorkflowTriggerResponse {
            trigger: TRIGGER_ASSET_TAGGED.to_string(),
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
        TRIGGER_ASSET_CREATE | TRIGGER_ASSET_METADATA | TRIGGER_ASSET_TAGGED => {
            &[TYPE_ASSET_V1][..]
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn result_constants_match_upstream_enum() {
        assert_eq!(WORKFLOW_RESULT_COMPLETED, "completed");
        assert_eq!(WORKFLOW_RESULT_HALTED, "halted");
        assert_eq!(WORKFLOW_RESULT_ERROR, "error");
    }

    #[test]
    fn logging_flag_gates_writes() {
        assert!(!should_write_workflow_log(false));
        assert!(should_write_workflow_log(true));
    }

    #[test]
    fn completed_log_omits_step_id() {
        assert_eq!(
            workflow_run_log(WorkflowRunEnd::Completed),
            WorkflowRunLog {
                result: WORKFLOW_RESULT_COMPLETED,
                include_step_id: false,
            }
        );
    }

    #[test]
    fn halted_and_error_logs_include_step_id() {
        assert_eq!(
            workflow_run_log(WorkflowRunEnd::Halted),
            WorkflowRunLog {
                result: WORKFLOW_RESULT_HALTED,
                include_step_id: true,
            }
        );
        assert_eq!(
            workflow_run_log(WorkflowRunEnd::Error),
            WorkflowRunLog {
                result: WORKFLOW_RESULT_ERROR,
                include_step_id: true,
            }
        );
    }

    #[test]
    fn asset_tagged_is_a_supported_trigger() {
        let triggers = get_workflow_triggers();
        assert!(
            triggers
                .iter()
                .any(|item| item.trigger == TRIGGER_ASSET_TAGGED)
        );
        assert!(is_method_compatible(
            &[TYPE_ASSET_V1.to_string()],
            TRIGGER_ASSET_TAGGED
        ));
        assert!(!is_method_compatible(
            &[TYPE_ASSET_V1.to_string()],
            "UnknownTrigger"
        ));
    }

    #[test]
    fn plugin_continue_defaults_to_true() {
        assert!(plugin_result_should_continue(&json!({})));
        assert!(plugin_result_should_continue(&json!({ "workflow": {} })));
        assert!(plugin_result_should_continue(
            &json!({ "workflow": { "continue": true } })
        ));
        assert!(!plugin_result_should_continue(
            &json!({ "workflow": { "continue": false } })
        ));
    }

    #[test]
    fn hostname_allowlist_matches_typescript_glob_rules() {
        let star = vec!["*".to_string()];
        assert!(hostname_matches_allowed_hosts("api.example.com", &star));

        let exact = vec!["api.example.com".to_string()];
        assert!(hostname_matches_allowed_hosts("api.example.com", &exact));
        assert!(!hostname_matches_allowed_hosts("evil.com", &exact));

        let wildcard = vec!["*.example.com".to_string()];
        assert!(hostname_matches_allowed_hosts("foo.example.com", &wildcard));
        assert!(!hostname_matches_allowed_hosts("example.org", &wildcard));

        assert!(!hostname_matches_allowed_hosts("api.example.com", &[]));
    }
}
