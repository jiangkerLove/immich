//! Legacy `search` queue worker.
//!
//! Upstream still starts a BullMQ worker for `QueueName.Search`, but no jobs are
//! registered on it (CLIP uses `smartSearch`). This no-op consumer keeps admin
//! queue stats parity and drains any orphan Redis jobs as `skipped`.

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde_json::Value;

use crate::models::dto::env::EnvDto;

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_SEARCH: &str = "search";

pub fn spawn(_env: EnvDto, redis_url: String, concurrency: usize) {
    tokio::spawn(async move {
        let worker = WorkerBuilder::new(QUEUE_SEARCH)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let job_name = job.name.clone();
                async move {
                    crate::service::workers::wrap_status_job(QUEUE_SEARCH, &job_name, || async {
                        eprintln!(
                            "search queue job {job_name} has no handler (legacy empty queue); skipping"
                        );
                        Ok::<_, String>("skipped")
                    })
                    .await
                }
            })
            .await;

        match handle {
            Ok(worker_handle) => {
                crate::service::worker_registry::register(worker_handle);
                std::future::pending::<()>().await;
            }
            Err(err) => {
                eprintln!("search worker failed to start: {err}");
            }
        }
    });
}
