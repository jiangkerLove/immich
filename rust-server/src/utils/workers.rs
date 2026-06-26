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

const DEFAULT_RUST_QUEUES: &[&str] = &[
    QUEUE_NOTIFICATIONS,
    QUEUE_BACKGROUND,
    QUEUE_BACKUP,
    QUEUE_METADATA,
    QUEUE_STORAGE_TEMPLATE,
    QUEUE_SIDECAR,
    QUEUE_SMART_SEARCH,
    QUEUE_THUMBNAIL,
    QUEUE_EDITOR,
    QUEUE_VIDEO,
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
pub fn should_run_microservices_workers(env: &EnvDto) -> bool {
    if list_contains(env.immich_workers_exclude.as_deref(), "microservices") {
        return false;
    }

    if let Some(include) = env.immich_workers_include.as_deref() {
        let workers = parse_worker_list(include);
        if workers.is_empty() {
            return true;
        }
        return workers.iter().any(|w| w == "microservices" || w == "api");
    }

    true
}

pub fn enabled_worker_queues(env: &EnvDto) -> Vec<&'static str> {
    if !should_run_microservices_workers(env) {
        return vec![QUEUE_NOTIFICATIONS];
    }

    DEFAULT_RUST_QUEUES.to_vec()
}
