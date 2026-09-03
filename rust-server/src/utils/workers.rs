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
    if is_maintenance_worker(env) {
        return vec![];
    }

    if !should_run_microservices_workers(env) {
        return vec![QUEUE_NOTIFICATIONS];
    }

    DEFAULT_RUST_QUEUES.to_vec()
}

pub fn is_maintenance_worker(env: &EnvDto) -> bool {
    list_contains(env.immich_workers_include.as_deref(), "maintenance")
        && !list_contains(env.immich_workers_include.as_deref(), "api")
}
