use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::response::response::ErrorResp;

const BULL_PREFIX: &str = "immich_bull";
const SIDECAR_QUEUE: &str = "sidecar";
const BACKGROUND_QUEUE: &str = "backgroundTask";

#[derive(Clone)]
pub struct JobService {
    redis_url: String,
}

#[derive(Serialize, Deserialize)]
struct SidecarWriteJob {
    id: Uuid,
}

#[derive(Serialize, Deserialize)]
struct EmptyTrashJob {}

impl JobService {
    pub fn new(redis_url: String) -> Self {
        Self { redis_url }
    }

    pub async fn queue_sidecar_write(&self, asset_id: &Uuid) -> Result<(), ErrorResp> {
        self.queue_sidecar_write_all(&[*asset_id]).await
    }

    pub async fn queue_sidecar_write_all(&self, asset_ids: &[Uuid]) -> Result<(), ErrorResp> {
        if asset_ids.is_empty() {
            return Ok(());
        }

        let queue = bullmq_rs::QueueBuilder::new(SIDECAR_QUEUE)
            .prefix(BULL_PREFIX)
            .connection(bullmq_rs::RedisConnection::new(self.redis_url.clone()))
            .build::<SidecarWriteJob>()
            .await
            .map_err(|err| ErrorResp::ServerError(format!("Failed to init job queue: {err}")))?;

        for asset_id in asset_ids {
            queue
                .add(
                    "SidecarWrite",
                    SidecarWriteJob { id: *asset_id },
                    None,
                )
                .await
                .map_err(|err| {
                    ErrorResp::ServerError(format!("Failed to queue SidecarWrite job: {err}"))
                })?;
        }

        Ok(())
    }

    pub async fn queue_asset_empty_trash(&self) -> Result<(), ErrorResp> {
        let queue = bullmq_rs::QueueBuilder::new(BACKGROUND_QUEUE)
            .prefix(BULL_PREFIX)
            .connection(bullmq_rs::RedisConnection::new(self.redis_url.clone()))
            .build::<EmptyTrashJob>()
            .await
            .map_err(|err| ErrorResp::ServerError(format!("Failed to init job queue: {err}")))?;

        queue
            .add("AssetEmptyTrash", EmptyTrashJob {}, None)
            .await
            .map_err(|err| {
                ErrorResp::ServerError(format!("Failed to queue AssetEmptyTrash job: {err}"))
            })?;

        Ok(())
    }
}
