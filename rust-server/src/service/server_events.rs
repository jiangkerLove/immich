//! Cross-process server events (mirrors Node `websocketRepository.serverSend`).
//!
//! When one rust-server instance updates system config, other instances receive
//! `ConfigUpdate` via Redis pub/sub and reload config / wake schedulers.

use std::sync::OnceLock;

use futures_util::StreamExt;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

const CONFIG_UPDATE_CHANNEL: &str = "immich:server:ConfigUpdate";

static INSTANCE_ID: OnceLock<String> = OnceLock::new();

fn instance_id() -> &'static str {
    INSTANCE_ID.get_or_init(|| Uuid::new_v4().to_string())
}

#[derive(Debug, Serialize, Deserialize)]
struct ConfigUpdateMessage {
    sender: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_config: Option<Value>,
}

pub fn spawn_listener(pool: PgPool, redis_url: String) {
    let self_id = instance_id().to_string();
    tokio::spawn(async move {
        let client = match redis::Client::open(redis_url.as_str()) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("server events: redis connect failed: {err}");
                return;
            }
        };

        let mut pubsub = match client.get_async_pubsub().await {
            Ok(value) => value,
            Err(err) => {
                eprintln!("server events: pubsub connect failed: {err}");
                return;
            }
        };

        if let Err(err) = pubsub.subscribe(CONFIG_UPDATE_CHANNEL).await {
            eprintln!("server events: subscribe failed: {err}");
            return;
        }

        println!("server events: listening for ConfigUpdate");

        let mut stream = pubsub.into_on_message();
        while let Some(msg) = stream.next().await {
            let payload: String = match msg.get_payload() {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("server events: payload error: {err}");
                    continue;
                }
            };

            let message: ConfigUpdateMessage = match serde_json::from_str(&payload) {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("server events: parse error: {err}");
                    continue;
                }
            };

            if message.sender == self_id {
                continue;
            }

            println!("server events: received ConfigUpdate from peer");
            crate::service::config_bootstrap::on_config_update(
                &pool,
                message.old_config.as_ref(),
            )
            .await;
        }

        eprintln!("server events: ConfigUpdate listener ended");
    });
}

pub async fn publish_config_update(redis_url: &str, old_config: Option<Value>) {
    let message = ConfigUpdateMessage {
        sender: instance_id().to_string(),
        old_config,
    };

    let payload = match serde_json::to_string(&message) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("server events: serialize failed: {err}");
            return;
        }
    };

    let client = match redis::Client::open(redis_url) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("server events: publish connect failed: {err}");
            return;
        }
    };

    let mut conn = match client.get_multiplexed_async_connection().await {
        Ok(value) => value,
        Err(err) => {
            eprintln!("server events: publish connection failed: {err}");
            return;
        }
    };

    if let Err(err) = conn
        .publish::<_, _, ()>(CONFIG_UPDATE_CHANNEL, payload)
        .await
    {
        eprintln!("server events: publish failed: {err}");
    }
}
