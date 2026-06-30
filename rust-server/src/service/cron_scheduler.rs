use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Local};
use sqlx::PgPool;

use crate::models::db::advisory_lock;
use crate::service::job::JobService;

pub type TickFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
type LastRunMap = HashMap<&'static str, DateTime<Local>>;

pub fn spawn_locked<F>(
    name: &'static str,
    lock_id: i64,
    pool: PgPool,
    jobs: JobService,
    bootstrap: Option<TickFuture>,
    mut on_tick: F,
) where
    F: FnMut(
            PgPool,
            JobService,
            DateTime<Local>,
            DateTime<Local>,
            Arc<Mutex<LastRunMap>>,
        ) -> TickFuture
        + Send
        + 'static,
{
    tokio::spawn(async move {
        let lock = match advisory_lock::try_acquire(&pool, lock_id).await {
            Ok(Some(value)) => value,
            Ok(None) => {
                println!("{name}: another instance holds the lock, skipping");
                return;
            }
            Err(err) => {
                eprintln!("{name}: failed to acquire lock: {err}");
                return;
            }
        };
        let _lock = lock;

        println!("{name}: started");

        if let Some(task) = bootstrap {
            task.await;
        }

        let last_run = Arc::new(Mutex::new(LastRunMap::new()));

        loop {
            crate::service::scheduler_notify::wait_or_notify(Duration::from_secs(60)).await;

            let now = Local::now();
            let since = now - chrono::Duration::seconds(59);
            on_tick(
                pool.clone(),
                jobs.clone(),
                now,
                since,
                last_run.clone(),
            )
            .await;
        }
    });
}

pub fn already_ran(
    last_run: &Arc<Mutex<LastRunMap>>,
    id: &'static str,
    since: DateTime<Local>,
) -> bool {
    last_run
        .lock()
        .ok()
        .is_some_and(|runs| runs.get(id).is_some_and(|previous| *previous >= since))
}

pub fn mark_ran(last_run: &Arc<Mutex<LastRunMap>>, id: &'static str, now: DateTime<Local>) {
    if let Ok(mut runs) = last_run.lock() {
        runs.insert(id, now);
    }
}
