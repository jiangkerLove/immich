use crate::models::dto::env::EnvDto;

pub const QUEUE_NOTIFICATIONS: &str = "notifications";
pub const QUEUE_BACKGROUND: &str = "backgroundTask";
pub const QUEUE_BACKUP: &str = "backupDatabase";
pub const QUEUE_THUMBNAIL: &str = "thumbnailGeneration";
pub const QUEUE_EDITOR: &str = "editor";
pub const QUEUE_VIDEO: &str = "videoConversion";
pub const QUEUE_METADATA: &str = "metadataExtraction";
pub const QUEUE_STORAGE_TEMPLATE: &str = "storageTemplateMigration";
pub const QUEUE_SIDECAR: &str = "sidecar";
pub const QUEUE_SMART_SEARCH: &str = "smartSearch";
pub const QUEUE_SEARCH: &str = "search";
pub const QUEUE_OCR: &str = "ocr";
pub const QUEUE_FACE: &str = "faceDetection";
pub const QUEUE_FACIAL: &str = "facialRecognition";
pub const QUEUE_DUPLICATE: &str = "duplicateDetection";
pub const QUEUE_MIGRATION: &str = "migration";
pub const QUEUE_INTEGRITY: &str = "integrityCheck";
pub const QUEUE_LIBRARY: &str = "library";
pub const QUEUE_WORKFLOW: &str = "workflow";

const DEFAULT_RUST_QUEUES: &[&str] = &[
    QUEUE_NOTIFICATIONS,
    QUEUE_BACKGROUND,
    QUEUE_BACKUP,
    QUEUE_METADATA,
    QUEUE_STORAGE_TEMPLATE,
    QUEUE_SIDECAR,
    QUEUE_SMART_SEARCH,
    QUEUE_SEARCH,
    QUEUE_OCR,
    QUEUE_FACE,
    QUEUE_FACIAL,
    QUEUE_DUPLICATE,
    QUEUE_MIGRATION,
    QUEUE_THUMBNAIL,
    QUEUE_EDITOR,
    QUEUE_VIDEO,
    QUEUE_INTEGRITY,
    QUEUE_LIBRARY,
    QUEUE_WORKFLOW,
];

fn parse_worker_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|part| part.trim().to_lowercase())
        .filter(|part| !part.is_empty())
        .collect()
}

fn list_contains(list: Option<&str>, worker: &str) -> bool {
    list.map(parse_worker_list)
        .is_some_and(|items| items.iter().any(|item| item == worker))
}

/// Whether this process should run BullMQ workers (microservices layer).
///
/// Matches Immich worker include semantics:
/// - unset include → run workers
/// - `INCLUDE=microservices` (optionally with `api`) → run workers
/// - `INCLUDE=api` alone → API-only process (no job workers / HLS encoder)
pub fn should_run_microservices_workers(env: &EnvDto) -> bool {
    if is_maintenance_worker(env) {
        return false;
    }

    if list_contains(env.immich_workers_exclude.as_deref(), "microservices") {
        return false;
    }

    if let Some(include) = env.immich_workers_include.as_deref() {
        let workers = parse_worker_list(include);
        if workers.is_empty() {
            return true;
        }
        return workers.iter().any(|w| w == "microservices");
    }

    true
}

/// Whether this process serves the HTTP API (including HLS waiters).
pub fn should_run_api(env: &EnvDto) -> bool {
    if is_maintenance_worker(env) {
        return false;
    }

    if list_contains(env.immich_workers_exclude.as_deref(), "api") {
        return false;
    }

    if let Some(include) = env.immich_workers_include.as_deref() {
        let workers = parse_worker_list(include);
        if workers.is_empty() {
            return true;
        }
        return workers.iter().any(|w| w == "api");
    }

    true
}

/// HLS ffmpeg / session ownership side (microservices process).
pub fn should_run_hls_worker(env: &EnvDto) -> bool {
    should_run_microservices_workers(env)
}

pub fn enabled_worker_queues(env: &EnvDto) -> Vec<&'static str> {
    if is_maintenance_worker(env) {
        return vec![];
    }

    if !should_run_microservices_workers(env) {
        // API-only still drains notification queue for websocket-adjacent mail jobs when needed.
        return vec![QUEUE_NOTIFICATIONS];
    }

    DEFAULT_RUST_QUEUES.to_vec()
}

pub fn is_maintenance_worker(env: &EnvDto) -> bool {
    list_contains(env.immich_workers_include.as_deref(), "maintenance")
        && !list_contains(env.immich_workers_include.as_deref(), "api")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::dto::env::EnvDto;

    fn env_with(include: Option<&str>, exclude: Option<&str>) -> EnvDto {
        let mut env = EnvDto::default();
        env.immich_workers_include = include.map(str::to_string);
        env.immich_workers_exclude = exclude.map(str::to_string);
        env
    }

    #[test]
    fn default_runs_both_api_and_workers() {
        let env = env_with(None, None);
        assert!(should_run_api(&env));
        assert!(should_run_microservices_workers(&env));
        assert!(should_run_hls_worker(&env));
    }

    #[test]
    fn include_api_only_disables_hls_worker() {
        let env = env_with(Some("api"), None);
        assert!(should_run_api(&env));
        assert!(!should_run_microservices_workers(&env));
        assert!(!should_run_hls_worker(&env));
    }

    #[test]
    fn include_microservices_only_disables_api_role() {
        let env = env_with(Some("microservices"), None);
        assert!(!should_run_api(&env));
        assert!(should_run_microservices_workers(&env));
        assert!(should_run_hls_worker(&env));
    }

    #[test]
    fn exclude_microservices_keeps_api() {
        let env = env_with(None, Some("microservices"));
        assert!(should_run_api(&env));
        assert!(!should_run_hls_worker(&env));
    }
}
