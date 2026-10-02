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

            metrics::gauge!("system.cpu.utilization")
                .set(f64::from(system.global_cpu_usage()) / 100.0);
            metrics::gauge!("system.memory.usage", "state" => "used")
                .set(system.used_memory() as f64);
            let free = system.total_memory().saturating_sub(system.used_memory());
            metrics::gauge!("system.memory.usage", "state" => "free").set(free as f64);

            if let Some(usage) = disk::check_disk_usage(&media_path) {
                metrics::gauge!("system.filesystem.usage", "state" => "used")
                    .set(usage.used as f64);
                metrics::gauge!("system.filesystem.usage", "state" => "free")
                    .set(usage.available as f64);
            }
        }
    });
}
