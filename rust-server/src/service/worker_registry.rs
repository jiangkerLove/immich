use std::sync::{Mutex, OnceLock};

use bullmq_rs::WorkerHandle;

static HANDLES: OnceLock<Mutex<Vec<WorkerHandle>>> = OnceLock::new();

pub fn register(handle: WorkerHandle) {
    HANDLES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .expect("worker registry mutex poisoned")
        .push(handle);
}

pub async fn shutdown_all() {
    let handles = HANDLES
        .get()
        .map(|lock| std::mem::take(&mut *lock.lock().expect("worker registry mutex poisoned")))
        .unwrap_or_default();

    if handles.is_empty() {
        return;
    }

    println!("Shutting down {} worker(s)...", handles.len());
    for handle in &handles {
        handle.shutdown();
    }
    for handle in handles {
        if let Err(err) = handle.wait().await {
            eprintln!("worker shutdown error: {err}");
        }
    }
}
