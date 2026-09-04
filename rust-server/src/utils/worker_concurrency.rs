use serde_json::Value;

use crate::utils::workers::{
    QUEUE_BACKGROUND, QUEUE_BACKUP, QUEUE_DUPLICATE, QUEUE_EDITOR, QUEUE_FACE, QUEUE_FACIAL,
    QUEUE_INTEGRITY, QUEUE_LIBRARY, QUEUE_METADATA, QUEUE_MIGRATION, QUEUE_NOTIFICATIONS,
    QUEUE_OCR, QUEUE_SEARCH, QUEUE_SIDECAR, QUEUE_SMART_SEARCH, QUEUE_STORAGE_TEMPLATE,
    QUEUE_THUMBNAIL, QUEUE_VIDEO, QUEUE_WORKFLOW,
};

const FIXED_CONCURRENCY_QUEUES: &[&str] = &[
    QUEUE_BACKUP,
    QUEUE_DUPLICATE,
    QUEUE_FACIAL,
    QUEUE_STORAGE_TEMPLATE,
];

pub fn config_key_for_queue(queue: &str) -> Option<&'static str> {
    Some(match queue {
        QUEUE_BACKGROUND => "backgroundTask",
        QUEUE_NOTIFICATIONS => "notification",
        QUEUE_THUMBNAIL => "thumbnailGeneration",
        QUEUE_EDITOR => "editor",
        QUEUE_VIDEO => "videoConversion",
        QUEUE_METADATA => "metadataExtraction",
        QUEUE_SIDECAR => "sidecar",
        QUEUE_SMART_SEARCH => "smartSearch",
        QUEUE_SEARCH => "search",
        QUEUE_OCR => "ocr",
        QUEUE_FACE => "faceDetection",
        QUEUE_MIGRATION => "migration",
        QUEUE_INTEGRITY => "integrityCheck",
        QUEUE_LIBRARY => "library",
        QUEUE_WORKFLOW => "workflow",
        _ => return None,
    })
}

pub fn concurrency_for_queue(config: &Value, queue: &str, default: usize) -> usize {
    if FIXED_CONCURRENCY_QUEUES.contains(&queue) {
        return 1;
    }

    let Some(key) = config_key_for_queue(queue) else {
        return default.max(1);
    };

    config
        .get("job")
        .and_then(|job| job.get(key))
        .and_then(|entry| entry.get("concurrency"))
        .and_then(|value| value.as_u64())
        .map(|value| value.max(1) as usize)
        .unwrap_or(default.max(1))
}

pub fn log_concurrency_settings(config: &Value) {
    let queues = [
        (QUEUE_BACKGROUND, 5usize),
        (QUEUE_NOTIFICATIONS, 5),
        (QUEUE_THUMBNAIL, 3),
        (QUEUE_EDITOR, 2),
        (QUEUE_VIDEO, 1),
        (QUEUE_METADATA, 5),
        (QUEUE_SIDECAR, 1),
        (QUEUE_SMART_SEARCH, 2),
        (QUEUE_SEARCH, 5),
        (QUEUE_OCR, 1),
        (QUEUE_FACE, 2),
        (QUEUE_FACIAL, 1),
        (QUEUE_DUPLICATE, 1),
        (QUEUE_MIGRATION, 1),
        (QUEUE_INTEGRITY, 2),
        (QUEUE_LIBRARY, 2),
        (QUEUE_WORKFLOW, 5),
        (QUEUE_STORAGE_TEMPLATE, 1),
        (QUEUE_BACKUP, 1),
    ];

    tracing::info!("worker concurrency: updating queue settings");
    for (queue, default) in queues {
        let value = concurrency_for_queue(config, queue, default);
        tracing::info!("worker concurrency: {queue} -> {value}");
    }
}
