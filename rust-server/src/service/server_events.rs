//! Cross-process server events (mirrors Node Socket.IO `serverSend` / CLI one-shot).
//!
//! Channels:
//! - `ConfigUpdate` — peer instances reload system config
//! - `AppRestart` — peer instances exit so the process manager restarts
//!   (maintenance enter/leave from UI or `immich-admin`)

use std::sync::OnceLock;

use futures_util::StreamExt;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

const CONFIG_UPDATE_CHANNEL: &str = "immich:server:ConfigUpdate";
const APP_RESTART_CHANNEL: &str = "immich:server:AppRestart";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRestartMessage {
    pub sender: String,
    pub is_maintenance_mode: bool,
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
            eprintln!("server events: subscribe ConfigUpdate failed: {err}");
            return;
        }
        if let Err(err) = pubsub.subscribe(APP_RESTART_CHANNEL).await {
            eprintln!("server events: subscribe AppRestart failed: {err}");
            return;
        }

        println!("server events: listening for ConfigUpdate + AppRestart");

        let mut stream = pubsub.into_on_message();
        while let Some(msg) = stream.next().await {
            let channel: String = match msg.get_channel() {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("server events: channel error: {err}");
                    continue;
                }
            };
            let payload: String = match msg.get_payload() {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("server events: payload error: {err}");
                    continue;
                }
            };

            match channel.as_str() {
                CONFIG_UPDATE_CHANNEL => {
                    handle_config_update(&pool, &self_id, &payload).await;
                }
                APP_RESTART_CHANNEL => {
                    handle_app_restart(&self_id, &payload);
                }
                other => {
                    eprintln!("server events: unexpected channel {other}");
                }
            }
        }

        eprintln!("server events: listener ended");
    });
}

async fn handle_config_update(pool: &PgPool, self_id: &str, payload: &str) {
    let message: ConfigUpdateMessage = match serde_json::from_str(payload) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("server events: ConfigUpdate parse error: {err}");
            return;
        }
    };

    if message.sender == self_id {
        return;
    }

    println!("server events: received ConfigUpdate from peer");
    crate::service::config_bootstrap::on_config_update(pool, message.old_config.as_ref()).await;
}

fn handle_app_restart(self_id: &str, payload: &str) {
    let message: AppRestartMessage = match serde_json::from_str(payload) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("server events: AppRestart parse error: {err}");
            return;
        }
    };

    if message.sender == self_id {
        return;
    }

    println!(
        "server events: received AppRestart (isMaintenanceMode={}); exiting",
        message.is_maintenance_mode
    );
    // Match TS AppRestart → process.exit (process manager restarts into the right mode).
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        std::process::exit(0);
    });
}

pub async fn publish_config_update(redis_url: &str, old_config: Option<Value>) {
    let message = ConfigUpdateMessage {
        sender: instance_id().to_string(),
        old_config,
    };

    if let Err(err) = publish_json(redis_url, CONFIG_UPDATE_CHANNEL, &message).await {
        eprintln!("server events: ConfigUpdate publish failed: {err}");
    }
}

/// Notify running rust-server process(es) to exit after DB maintenance flag changed.
pub async fn publish_app_restart(redis_url: &str, is_maintenance_mode: bool) -> Result<(), String> {
    let message = AppRestartMessage {
        sender: instance_id().to_string(),
        is_maintenance_mode,
    };
    publish_json(redis_url, APP_RESTART_CHANNEL, &message).await
}

async fn publish_json<T: Serialize>(
    redis_url: &str,
    channel: &str,
    message: &T,
) -> Result<(), String> {
    let payload = serde_json::to_string(message).map_err(|err| err.to_string())?;

    let start = std::time::Instant::now();
    let client = redis::Client::open(redis_url).map_err(|err| err.to_string())?;
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .map_err(|err| err.to_string())?;

    let result = conn
        .publish::<_, _, ()>(channel, payload)
        .await
        .map_err(|err| err.to_string());
    crate::utils::telemetry::record_redis_command(
        "publish",
        start.elapsed().as_secs_f64() * 1000.0,
        result.is_ok(),
    );
    result
}

/// Build `redis://…` URL from env (same rules as `AppState`).
pub fn redis_url_from_env(settings: &crate::models::dto::env::EnvDto) -> String {
    let has_user = settings
        .redis_username
        .as_ref()
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let has_pass = settings
        .redis_password
        .as_ref()
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    if has_user && has_pass {
        format!(
            "redis://{}:{}@{}:{}",
            settings.redis_username.as_ref().unwrap(),
            settings.redis_password.as_ref().unwrap(),
            settings.redis_hostname,
            settings.redis_port
        )
    } else {
        format!(
            "redis://{}:{}",
            settings.redis_hostname, settings.redis_port
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_restart_message_roundtrip() {
        let message = AppRestartMessage {
            sender: "cli".into(),
            is_maintenance_mode: true,
        };
        let json = serde_json::to_string(&message).unwrap();
        let parsed: AppRestartMessage = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_maintenance_mode);
        assert_eq!(parsed.sender, "cli");
    }
}
