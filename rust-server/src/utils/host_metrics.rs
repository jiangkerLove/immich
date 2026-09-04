use std::path::PathBuf;
use std::time::Duration;

use sysinfo::System;

use crate::utils::disk;
use crate::utils::telemetry;

pub fn spawn_collector(media_path: PathBuf) {
    if !telemetry::host_metrics_enabled() {
        return;
    }

    tokio::spawn(async move {
        let mut system = System::new();
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            if !telemetry::host_metrics_enabled() {
                continue;
            }

            system.refresh_cpu_usage();
            system.refresh_memory();

            metrics::gauge!("immich.host.cpu.usage_percent")
                .set(f64::from(system.global_cpu_usage()));
            metrics::gauge!("immich.host.memory.used_bytes").set(system.used_memory() as f64);
            metrics::gauge!("immich.host.memory.total_bytes").set(system.total_memory() as f64);

            if let Some(usage) = disk::check_disk_usage(&media_path) {
                metrics::gauge!("immich.host.disk.used_bytes").set(usage.used as f64);
                metrics::gauge!("immich.host.disk.available_bytes").set(usage.available as f64);
                metrics::gauge!("immich.host.disk.total_bytes").set(usage.total as f64);
            }
        }
    });
}
