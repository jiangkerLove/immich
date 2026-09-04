//! Periodic sqlx pool gauges when `IMMICH_TELEMETRY_INCLUDE` contains `repo`.

use std::time::Duration;

use sqlx::PgPool;

use crate::utils::telemetry;

pub fn spawn_pool_collector(pool: PgPool, max_connections: u32) {
    if !telemetry::repo_metrics_enabled() {
        return;
    }

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            if !telemetry::repo_metrics_enabled() {
                continue;
            }

            telemetry::record_db_pool_stats(pool.size(), pool.num_idle(), max_connections);
        }
    });
}
