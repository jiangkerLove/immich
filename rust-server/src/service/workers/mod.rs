use std::sync::OnceLock;

use serde_json::Value;
use sqlx::PgPool;

use crate::models::dto::env::EnvDto;
use crate::service::job::JobService;
use crate::service::websocket::WebSocketHub;
use crate::utils::storage::StoragePaths;
use crate::utils::system_config::{defaults, get_merged};
use crate::utils::worker_concurrency::concurrency_for_queue;
use crate::utils::workers::{
    QUEUE_BACKGROUND, QUEUE_BACKUP, QUEUE_DUPLICATE, QUEUE_EDITOR, QUEUE_FACE, QUEUE_FACIAL,
    QUEUE_INTEGRITY, QUEUE_LIBRARY, QUEUE_METADATA, QUEUE_MIGRATION, QUEUE_NOTIFICATIONS,
    QUEUE_OCR, QUEUE_SEARCH, QUEUE_SIDECAR, QUEUE_SMART_SEARCH, QUEUE_STORAGE_TEMPLATE,
    QUEUE_THUMBNAIL, QUEUE_VIDEO, QUEUE_WORKFLOW, enabled_worker_queues,
};

mod background_task;
mod backup_database;
mod duplicate_detection;
mod editor;
mod face_detection;
mod facial_recognition;
mod integrity;
mod job_handler;
mod library;
mod metadata_extraction;
mod migration;
mod notifications;
mod ocr;
mod search;
mod sidecar;
mod smart_search;
mod storage_template_migration;
mod thumbnail_generation;
mod video_conversion;
mod workflow;

pub use job_handler::{
    begin_job, end_job, end_job_with_status, finish_failed, finish_ok, worker_error,
    wrap_simple_job, wrap_status_job,
};

static WORKER_CTX: OnceLock<WorkerContext> = OnceLock::new();

#[derive(Clone)]
pub struct WorkerContext {
    pub pool: PgPool,
    pub redis_url: String,
    pub storage: StoragePaths,
    pub env: EnvDto,
    pub websocket: WebSocketHub,
    pub jobs: JobService,
}

pub fn spawn_all(ctx: WorkerContext) {
    let _ = WORKER_CTX.set(ctx.clone());

    if crate::utils::workers::should_run_microservices_workers(&ctx.env)
        && crate::utils::telemetry::job_metrics_enabled()
    {
        if let Some(config) = crate::utils::telemetry::config() {
            if !crate::utils::telemetry::api_metrics_enabled() {
                crate::utils::telemetry::spawn_prometheus_exporter(config.microservices_port);
                tracing::info!(
                    "prometheus job metrics listening on 0.0.0.0:{}",
                    config.microservices_port
                );
            }
        }
    }

    tokio::spawn(async move {
        if crate::utils::workers::should_run_microservices_workers(&ctx.env) {
            if let Err(err) = crate::service::plugin_import::sync_plugins(&ctx.pool, &ctx.env).await
            {
                tracing::error!("plugin import failed: {err}");
            }
        }
        let config = get_merged(&ctx.pool).await.unwrap_or_else(|_| defaults());
        spawn_workers(&ctx, &config).await;
    });
}

pub async fn restart_all(config: &Value) {
    let Some(ctx) = WORKER_CTX.get() else {
        return;
    };

    crate::utils::worker_concurrency::log_concurrency_settings(config);
    crate::service::worker_registry::shutdown_all().await;
    spawn_workers(ctx, config).await;
}

async fn spawn_workers(ctx: &WorkerContext, config: &Value) {
    crate::service::media::exiftool::configure_concurrency(concurrency_for_queue(
        config,
        QUEUE_METADATA,
        5,
    ))
    .await;

    for queue in enabled_worker_queues(&ctx.env) {
        match queue {
            QUEUE_NOTIFICATIONS => notifications::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.websocket.clone(),
                ctx.jobs.clone(),
                concurrency_for_queue(config, queue, 5),
            ),
            QUEUE_BACKGROUND => background_task::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.websocket.clone(),
                ctx.env.clone(),
                ctx.storage.clone(),
                ctx.jobs.clone(),
                concurrency_for_queue(config, queue, 5),
            ),
            QUEUE_BACKUP => backup_database::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
                ctx.websocket.clone(),
                concurrency_for_queue(config, queue, 1),
            ),
            QUEUE_THUMBNAIL => thumbnail_generation::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
                concurrency_for_queue(config, queue, 3),
            ),
            QUEUE_EDITOR => editor::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
                concurrency_for_queue(config, queue, 2),
            ),
            QUEUE_VIDEO => video_conversion::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
                concurrency_for_queue(config, queue, 1),
            ),
            QUEUE_METADATA => metadata_extraction::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
                ctx.websocket.clone(),
                concurrency_for_queue(config, queue, 5),
            ),
            QUEUE_STORAGE_TEMPLATE => storage_template_migration::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
                concurrency_for_queue(config, queue, 1),
            ),
            QUEUE_SIDECAR => sidecar::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.env.clone(),
                concurrency_for_queue(config, queue, 1),
            ),
            QUEUE_SMART_SEARCH => smart_search::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.env.clone(),
                concurrency_for_queue(config, queue, 2),
            ),
            QUEUE_SEARCH => search::spawn(
                ctx.env.clone(),
                ctx.redis_url.clone(),
                concurrency_for_queue(config, queue, 5),
            ),
            QUEUE_OCR => ocr::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.env.clone(),
                concurrency_for_queue(config, queue, 1),
            ),
            QUEUE_FACE => face_detection::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.env.clone(),
                concurrency_for_queue(config, queue, 2),
            ),
            QUEUE_FACIAL => facial_recognition::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.env.clone(),
                concurrency_for_queue(config, queue, 1),
            ),
            QUEUE_DUPLICATE => duplicate_detection::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.env.clone(),
                concurrency_for_queue(config, queue, 1),
            ),
            QUEUE_INTEGRITY => integrity::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.jobs.clone(),
                concurrency_for_queue(config, queue, 2),
            ),
            QUEUE_LIBRARY => library::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.jobs.clone(),
                concurrency_for_queue(config, queue, 2),
            ),
            QUEUE_MIGRATION => migration::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
                concurrency_for_queue(config, queue, 1),
            ),
            QUEUE_WORKFLOW => workflow::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.env.clone(),
                ctx.jobs.clone(),
                ctx.websocket.clone(),
                concurrency_for_queue(config, queue, 5),
            ),
            _ => {}
        }
    }

    let queues = enabled_worker_queues(&ctx.env);
    if queues.len() > 1 {
        tracing::info!(
            "rust microservices workers enabled for queues: {}",
            queues.join(", ")
        );
    }
}
