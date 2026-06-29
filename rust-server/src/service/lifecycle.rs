use std::sync::{Arc, OnceLock};

use crate::service::library_watcher;
use crate::service::transcoding::HlsEngine;
use crate::service::worker_registry;

static HLS_ENGINE: OnceLock<Arc<HlsEngine>> = OnceLock::new();

pub fn register_hls_engine(engine: Arc<HlsEngine>) {
    let _ = HLS_ENGINE.set(engine);
}

pub async fn on_shutdown() {
    if let Some(engine) = HLS_ENGINE.get() {
        engine.shutdown().await;
    }
    library_watcher::shutdown();
    worker_registry::shutdown_all().await;
    crate::service::ml_health::teardown();
}
