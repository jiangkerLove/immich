mod background_task;
mod backup_database;
mod editor;
mod notifications;
mod thumbnail_generation;
mod metadata_extraction;
mod storage_template_migration;
mod sidecar;
mod smart_search;
mod video_conversion;

use sqlx::PgPool;

use crate::models::dto::env::EnvDto;
use crate::service::job::JobService;
use crate::service::websocket::WebSocketHub;
use crate::utils::storage::StoragePaths;
use crate::utils::workers::{
    enabled_worker_queues, QUEUE_BACKGROUND, QUEUE_BACKUP, QUEUE_EDITOR, QUEUE_METADATA,
    QUEUE_NOTIFICATIONS, QUEUE_SIDECAR, QUEUE_SMART_SEARCH, QUEUE_STORAGE_TEMPLATE,
    QUEUE_THUMBNAIL, QUEUE_VIDEO,
};

pub struct WorkerContext {
    pub pool: PgPool,
    pub redis_url: String,
    pub storage: StoragePaths,
    pub env: EnvDto,
    pub websocket: WebSocketHub,
    pub jobs: JobService,
}

pub fn spawn_all(ctx: WorkerContext) {
    for queue in enabled_worker_queues(&ctx.env) {
        match queue {
            QUEUE_NOTIFICATIONS => notifications::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.websocket.clone(),
                ctx.jobs.clone(),
            ),
            QUEUE_BACKGROUND => {
                background_task::spawn(ctx.pool.clone(), ctx.redis_url.clone())
            }
            QUEUE_BACKUP => backup_database::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
            ),
            QUEUE_THUMBNAIL => thumbnail_generation::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
            ),
            QUEUE_EDITOR => editor::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
            ),
            QUEUE_VIDEO => video_conversion::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
            ),
            QUEUE_METADATA => metadata_extraction::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.env.clone(),
            ),
            QUEUE_STORAGE_TEMPLATE => storage_template_migration::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.storage.clone(),
                ctx.env.clone(),
            ),
            QUEUE_SIDECAR => sidecar::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.env.clone(),
            ),
            QUEUE_SMART_SEARCH => smart_search::spawn(
                ctx.pool.clone(),
                ctx.redis_url.clone(),
                ctx.env.clone(),
            ),
            _ => {}
        }
    }

    let queues = enabled_worker_queues(&ctx.env);
    if queues.len() > 1 {
        println!(
            "rust microservices workers enabled for queues: {}",
            queues.join(", ")
        );
    }
}
