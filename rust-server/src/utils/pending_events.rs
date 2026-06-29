use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, oneshot};

#[derive(Clone)]
pub struct PendingEvents<T> {
    timeout_ms: u64,
    pending: Arc<Mutex<HashMap<String, Vec<oneshot::Sender<Result<T, String>>>>>>,
}

impl<T: Send + Clone + 'static> PendingEvents<T> {
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            timeout_ms,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn wait(&self, key: String) -> Result<T, String> {
        let (tx, rx) = oneshot::channel();
        {
            let mut guard = self.pending.lock().await;
            guard.entry(key.clone()).or_default().push(tx);
        }

        let pending = self.pending.clone();
        let timeout_key = key.clone();
        let timeout_ms = self.timeout_ms;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
            let mut guard = pending.lock().await;
            if let Some(waiters) = guard.remove(&timeout_key) {
                for waiter in waiters {
                    let _ = waiter.send(Err("Request timed out".to_string()));
                }
            }
        });

        rx.await
            .map_err(|_| "Request timed out".to_string())?
    }

    pub async fn complete(&self, key: &str, value: T) {
        let waiters = self.pending.lock().await.remove(key);
        if let Some(waiters) = waiters {
            for waiter in waiters {
                let _ = waiter.send(Ok(value.clone()));
            }
        }
    }

    pub async fn reject(&self, key: &str, error: impl Into<String>) {
        let waiters = self.pending.lock().await.remove(key);
        let message = error.into();
        if let Some(waiters) = waiters {
            for waiter in waiters {
                let _ = waiter.send(Err(message.clone()));
            }
        }
    }

    pub async fn reject_by_prefix(&self, prefix: &str, error: impl Into<String>) {
        let message = error.into();
        let keys: Vec<String> = self
            .pending
            .lock()
            .await
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect();
        for key in keys {
            self.reject(&key, message.clone()).await;
        }
    }
}
