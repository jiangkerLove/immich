//! Periodic Redis PING latency when `IMMICH_TELEMETRY_INCLUDE` contains `io`.

use std::time::{Duration, Instant};

use crate::utils::telemetry;

pub fn spawn_redis_collector(redis_url: String) {
    if !telemetry::io_metrics_enabled() {
        return;
    }

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            if !telemetry::io_metrics_enabled() {
                continue;
            }

            let start = Instant::now();
            let result = ping_redis(&redis_url).await;
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            telemetry::record_redis_command("ping", elapsed_ms, result.is_ok());
            if let Err(err) = result {
                eprintln!("io metrics: redis ping failed: {err}");
            }
        }
    });
}

async fn ping_redis(redis_url: &str) -> Result<(), String> {
    let client = redis::Client::open(redis_url).map_err(|err| err.to_string())?;
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .map_err(|err| err.to_string())?;
    redis::cmd("PING")
        .query_async::<String>(&mut conn)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}
