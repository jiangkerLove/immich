use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::Notify;

static NOTIFY: OnceLock<Notify> = OnceLock::new();

fn notify() -> &'static Notify {
    NOTIFY.get_or_init(Notify::new)
}

/// Wake all cron schedulers so they re-read config immediately (mirrors Node `cronRepository.update`).
pub fn wake_all() {
    notify().notify_waiters();
}

pub async fn wait_or_notify(interval: Duration) {
    tokio::select! {
        _ = tokio::time::sleep(interval) => {}
        _ = notify().notified() => {}
    }
}
