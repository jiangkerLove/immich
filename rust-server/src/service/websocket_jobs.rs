use std::collections::HashMap;

use bullmq_rs::queue_events::{QueueEvent, QueueEventsBuilder};
use bullmq_rs::{QueueBuilder, RedisConnection};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::asset_edit;
use crate::models::db::assets;
use crate::models::response::asset::get_asset_response;
use crate::models::response::sync::{
    AssetEditReadyV2, AssetUploadReadyV2, SyncAssetEditV1, SyncAssetV2, sync_exif_from_json,
};
use crate::service::job::PersonJob;
use crate::service::websocket::WebSocketHub;

const BULL_PREFIX: &str = "immich_bull";

const QUEUE_THUMBNAIL: &str = "thumbnailGeneration";
const QUEUE_EDITOR: &str = "editor";
const QUEUE_METADATA: &str = "metadataExtraction";

#[derive(Debug, Deserialize)]
struct EntityJobData {
    id: Uuid,
    source: Option<String>,
    notify: Option<bool>,
}

pub struct WebSocketJobListener;

impl WebSocketJobListener {
    pub fn spawn(pool: PgPool, redis_url: String, websocket: WebSocketHub) {
        for queue_name in [QUEUE_THUMBNAIL, QUEUE_EDITOR, QUEUE_METADATA] {
            let pool = pool.clone();
            let redis_url = redis_url.clone();
            let websocket = websocket.clone();
            tokio::spawn(async move {
                if let Err(err) = listen_queue(pool, redis_url, websocket, queue_name).await {
                    tracing::error!("websocket job listener ({queue_name}): {err}");
                }
            });
        }
    }
}

async fn listen_queue(
    pool: PgPool,
    redis_url: String,
    websocket: WebSocketHub,
    queue_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = RedisConnection::new(redis_url.clone());
    let events = QueueEventsBuilder::new(queue_name)
        .prefix(BULL_PREFIX)
        .connection(conn.clone())
        .build()
        .await?;

    let queue = QueueBuilder::new(queue_name)
        .prefix(BULL_PREFIX)
        .connection(RedisConnection::new(redis_url))
        .build::<Value>()
        .await?;

    let mut rx = events.subscribe();
    let mut job_names: HashMap<String, String> = HashMap::new();

    loop {
        let Ok((event, _)) = rx.recv().await else {
            break;
        };

        match event {
            QueueEvent::Added { job_id, name } => {
                job_names.insert(job_id, name);
            }
            QueueEvent::Completed {
                job_id,
                return_value,
            } => {
                if !is_job_success(&return_value) {
                    job_names.remove(&job_id);
                    continue;
                }

                let job_name = job_names.remove(&job_id).or_else(|| {
                    // fallback: job may have been added before listener started
                    None
                });

                let job = queue.get_job(&job_id).await.ok().flatten();
                let (name, data) = match (job_name, job) {
                    (Some(name), Some(job)) => (name, job.data),
                    (_, Some(job)) => (job.name, job.data),
                    (Some(name), None) => {
                        if let Ok(id) = Uuid::parse_str(&job_id) {
                            let data = serde_json::json!({ "id": id });
                            (name, data)
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };

                handle_job_completed(&pool, &websocket, queue_name, &name, &data).await;
            }
            _ => {}
        }
    }

    Ok(())
}

fn is_job_success(return_value: &Value) -> bool {
    match return_value {
        Value::String(s) => {
            let lower = s.to_ascii_lowercase();
            lower == "success" || lower == "skipped"
        }
        Value::Null => true,
        _ => true,
    }
}

async fn handle_job_completed(
    pool: &PgPool,
    websocket: &WebSocketHub,
    queue_name: &str,
    job_name: &str,
    data: &Value,
) {
    match (queue_name, job_name) {
        (QUEUE_THUMBNAIL, "AssetGenerateThumbnails") => {
            let Ok(job) = serde_json::from_value::<EntityJobData>(data.clone()) else {
                return;
            };
            handle_asset_generate_thumbnails(pool, websocket, &job).await;
        }
        (QUEUE_THUMBNAIL, "PersonGenerateThumbnail") => {
            let Ok(job) = serde_json::from_value::<PersonJob>(data.clone()) else {
                return;
            };
            websocket.emit_person_thumbnail(job.owner_id, job.person_group_id);
        }
        (QUEUE_EDITOR, "AssetEditThumbnailGeneration") => {
            let Ok(job) = serde_json::from_value::<EntityJobData>(data.clone()) else {
                return;
            };
            handle_asset_edit_ready(pool, websocket, job.id).await;
        }
        (QUEUE_METADATA, "AssetExtractMetadata") => {
            let Ok(job) = serde_json::from_value::<EntityJobData>(data.clone()) else {
                return;
            };
            if job.source.as_deref() == Some("sidecar-write") {
                handle_asset_metadata_sidecar(pool, websocket, job.id).await;
            }
        }
        _ => {}
    }
}

async fn handle_asset_generate_thumbnails(
    pool: &PgPool,
    websocket: &WebSocketHub,
    job: &EntityJobData,
) {
    if !job.notify.unwrap_or(false) && job.source.as_deref() != Some("upload") {
        return;
    }

    let Ok(Some(row)) = assets::get_detail_by_id(pool, &job.id).await else {
        return;
    };

    if row.visibility != "timeline" && row.visibility != "archive" {
        return;
    }

    let Ok(Some(asset)) = get_asset_response(pool, &job.id).await else {
        return;
    };

    websocket.emit_upload_success(row.owner_id, asset.clone());

    if let Some(exif_json) = row.exif_json.as_ref() {
        let payload = AssetUploadReadyV2 {
            asset: SyncAssetV2::from(&row),
            exif: sync_exif_from_json(row.id, exif_json),
        };
        websocket.emit_asset_upload_ready(row.owner_id, payload);
    }
}

async fn handle_asset_edit_ready(pool: &PgPool, websocket: &WebSocketHub, asset_id: Uuid) {
    let Ok(Some(row)) = assets::get_detail_by_id(pool, &asset_id).await else {
        return;
    };

    let Ok(edits) = asset_edit::list_by_asset(pool, &asset_id).await else {
        return;
    };

    let payload = AssetEditReadyV2 {
        asset: SyncAssetV2::from(&row),
        edit: edits
            .into_iter()
            .map(|edit| SyncAssetEditV1 {
                id: edit.id,
                asset_id,
                action: edit.action,
                parameters: edit.parameters,
            })
            .collect(),
    };

    websocket.emit_asset_edit_ready(row.owner_id, payload);
}

async fn handle_asset_metadata_sidecar(pool: &PgPool, websocket: &WebSocketHub, asset_id: Uuid) {
    let Ok(Some(asset)) = get_asset_response(pool, &asset_id).await else {
        return;
    };

    websocket.emit_asset_update(asset.owner_id, asset);
}
