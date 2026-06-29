use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Client;
use serde_json::Value;
use sqlx::PgPool;

use crate::utils::system_config::{get_merged, json_bool, json_i32, json_str};

struct MlHealthState {
    healthy: std::collections::HashMap<String, bool>,
    interval_handle: Option<tokio::task::JoinHandle<()>>,
}

static STATE: std::sync::OnceLock<Arc<Mutex<MlHealthState>>> = std::sync::OnceLock::new();

pub async fn setup(pool: &PgPool) {
    teardown();

    let config = match get_merged(pool).await {
        Ok(value) => value,
        Err(err) => {
            eprintln!("ml health: failed to load config: {err}");
            return;
        }
    };

    let ml = config.get("machineLearning").cloned().unwrap_or_default();
    if !json_bool(&ml, &["enabled"], false)
        || !json_bool(&ml, &["availabilityChecks", "enabled"], false)
    {
        return;
    }

    let urls: Vec<String> = ml
        .get("urls")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if urls.is_empty() {
        return;
    }

    let timeout_ms = json_i32(&ml, &["availabilityChecks", "timeout"], 2000).max(1) as u64;
    let interval_ms = json_i32(&ml, &["availabilityChecks", "interval"], 30_000).max(1) as u64;

    let state = Arc::new(Mutex::new(MlHealthState {
        healthy: std::collections::HashMap::new(),
        interval_handle: None,
    }));
    let _ = STATE.set(state.clone());

    {
        let urls = urls.clone();
        let state = state.clone();
        tick(&urls, timeout_ms, &state).await;
        let state_for_task = state.clone();
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms));
            loop {
                ticker.tick().await;
                tick(&urls, timeout_ms, &state_for_task).await;
            }
        });
        state.lock().expect("ml health mutex poisoned").interval_handle = Some(handle);
    }

    println!(
        "ml health: started availability checks for {} url(s)",
        urls.len()
    );
}

pub fn teardown() {
    if let Some(state) = STATE.get() {
        if let Ok(mut guard) = state.lock() {
            if let Some(handle) = guard.interval_handle.take() {
                handle.abort();
            }
            guard.healthy.clear();
        }
    }
}

pub fn is_healthy(url: &str) -> bool {
    STATE
        .get()
        .and_then(|state| state.lock().ok())
        .and_then(|guard| guard.healthy.get(url).copied())
        .unwrap_or(true)
}

async fn tick(urls: &[String], timeout_ms: u64, state: &Arc<Mutex<MlHealthState>>) {
    let client = Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .unwrap_or_else(|_| Client::new());

    for url in urls {
        let endpoint = format!("{}/ping", url.trim_end_matches('/'));
        let healthy = client
            .get(&endpoint)
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false);

        if let Ok(mut guard) = state.lock() {
            let previous = guard.healthy.insert(url.clone(), healthy);
            if previous != Some(healthy) {
                println!(
                    "ml health: {url} is {}",
                    if healthy { "healthy" } else { "unhealthy" }
                );
            }
        }
    }
}

pub fn filter_healthy_urls(urls: &[String]) -> Vec<String> {
    urls.iter()
        .filter(|url| is_healthy(url.as_str()))
        .cloned()
        .collect()
}

pub fn log_level_from_config(env_level: Option<&str>, config: &Value) -> String {
    if let Some(level) = env_level.filter(|value| !value.is_empty()) {
        return level.to_string();
    }

    let logging = config.get("logging").cloned().unwrap_or_default();
    if !json_bool(&logging, &["enabled"], true) {
        return "false".to_string();
    }

    json_str(&logging, &["level"], "log")
}
