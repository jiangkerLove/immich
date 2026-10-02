use std::collections::HashSet;

use crate::models::dto::env::EnvDto;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImmichTelemetry {
    Host,
    Api,
    Io,
    Repo,
    Job,
}

#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub metrics: HashSet<ImmichTelemetry>,
    pub api_port: u16,
    pub microservices_port: u16,
}

static ENABLED: std::sync::OnceLock<TelemetryConfig> = std::sync::OnceLock::new();

pub fn init(env: &EnvDto) -> TelemetryConfig {
    let config = parse_telemetry(env);
    let _ = ENABLED.set(config.clone());
    config
}

pub fn config() -> Option<&'static TelemetryConfig> {
    ENABLED.get()
}

pub fn metrics_enabled() -> bool {
    ENABLED
        .get()
        .is_some_and(|config| !config.metrics.is_empty())
}

pub fn api_metrics_enabled() -> bool {
    ENABLED
        .get()
        .is_some_and(|config| config.metrics.contains(&ImmichTelemetry::Api))
}

pub fn job_metrics_enabled() -> bool {
    ENABLED
        .get()
        .is_some_and(|config| config.metrics.contains(&ImmichTelemetry::Job))
}

pub fn host_metrics_enabled() -> bool {
    ENABLED
        .get()
        .is_some_and(|config| config.metrics.contains(&ImmichTelemetry::Host))
}

pub fn repo_metrics_enabled() -> bool {
    ENABLED
        .get()
        .is_some_and(|config| config.metrics.contains(&ImmichTelemetry::Repo))
}

pub fn io_metrics_enabled() -> bool {
    ENABLED
        .get()
        .is_some_and(|config| config.metrics.contains(&ImmichTelemetry::Io))
}

pub fn parse_telemetry(env: &EnvDto) -> TelemetryConfig {
    let all = [
        ImmichTelemetry::Host,
        ImmichTelemetry::Api,
        ImmichTelemetry::Io,
        ImmichTelemetry::Repo,
        ImmichTelemetry::Job,
    ];

    let mut included: HashSet<ImmichTelemetry> =
        if env.immich_telemetry_include.as_deref() == Some("all") {
            all.into_iter().collect()
        } else {
            parse_telemetry_list(env.immich_telemetry_include.as_deref())
        };

    for item in parse_telemetry_list(env.immich_telemetry_exclude.as_deref()) {
        included.remove(&item);
    }

    TelemetryConfig {
        metrics: included,
        api_port: env.immich_api_metrics_port.unwrap_or(8081),
        microservices_port: env.immich_microservices_metrics_port.unwrap_or(8082),
    }
}

fn parse_telemetry_list(value: Option<&str>) -> HashSet<ImmichTelemetry> {
    let Some(raw) = value else {
        return HashSet::new();
    };

    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter_map(parse_telemetry_item)
        .collect()
}

fn parse_telemetry_item(value: &str) -> Option<ImmichTelemetry> {
    match value.to_ascii_lowercase().as_str() {
        "host" => Some(ImmichTelemetry::Host),
        "api" => Some(ImmichTelemetry::Api),
        "io" => Some(ImmichTelemetry::Io),
        "repo" => Some(ImmichTelemetry::Repo),
        "job" => Some(ImmichTelemetry::Job),
        _ => None,
    }
}

pub fn spawn_prometheus_exporter(port: u16) {
    std::thread::spawn(move || {
        if let Err(err) = metrics_exporter_prometheus::PrometheusBuilder::new()
            .with_http_listener(([0, 0, 0, 0], port))
            .install()
        {
            tracing::error!("prometheus metrics exporter failed on port {port}: {err}");
        }
    });
}

pub fn record_http_request(duration_ms: f64, status: u16) {
    if !api_metrics_enabled() {
        return;
    }
    metrics::histogram!(
        "http.server.request.duration",
        "http.response.status_code" => status.to_string()
    )
    .record(duration_ms / 1000.0);
}

pub fn record_job_finished(_queue: &str, _job_name: &str, _success: bool) {}

pub fn record_job_started(_queue: &str, _job_name: &str) {}

pub fn record_queue_started(queue: &str) {
    if !job_metrics_enabled() {
        return;
    }
    let name = metric_name(&format!("immich.queues.{}.started", snake_case(queue)));
    metrics::counter!(name).increment(1);
}

pub fn record_queue_active_delta(queue: &str, delta: i64) {
    if !job_metrics_enabled() {
        return;
    }
    let name = metric_name(&format!("immich.queues.{}.active", snake_case(queue)));
    if delta >= 0 {
        metrics::gauge!(name).increment(delta as f64);
    } else {
        metrics::gauge!(name).decrement((-delta) as f64);
    }
}

pub fn record_job_status(job_name: &str, status: &str) {
    if !job_metrics_enabled() {
        return;
    }
    if !matches!(status, "success" | "failed" | "skipped") {
        return;
    }
    let name = metric_name(&format!("immich.jobs.{}.{}", snake_case(job_name), status));
    metrics::counter!(name).increment(1);
}

/// Repository / DB layer duration (mirrors TS `repo` method histograms).
pub fn record_repo_duration(operation: &str, duration_ms: f64) {
    if !repo_metrics_enabled() {
        return;
    }
    let operation = sanitize_metric_name(operation);
    metrics::histogram!(
        "db.client.operation.duration",
        "db.system.name" => "postgresql",
        "db.operation.name" => operation
    )
    .record(duration_ms / 1000.0);
}

pub fn record_db_pool_stats(size: u32, idle: usize, max: u32) {
    if !repo_metrics_enabled() {
        return;
    }
    let used = size.saturating_sub(u32::try_from(idle).unwrap_or(0));
    metrics::gauge!(
        "db.client.connection.count",
        "db.system.name" => "postgresql",
        "state" => "used"
    )
    .set(f64::from(used));
    metrics::gauge!(
        "db.client.connection.count",
        "db.system.name" => "postgresql",
        "state" => "idle"
    )
    .set(idle as f64);
    let _ = max;
}

/// Redis / IO layer (mirrors TS IORedis instrumentation behind `io`).
pub fn record_redis_command(operation: &str, duration_ms: f64, success: bool) {
    if !io_metrics_enabled() {
        return;
    }
    let operation = sanitize_metric_name(operation);
    let status = if success { "ok" } else { "error" };
    metrics::histogram!(
        "db.client.operation.duration",
        "db.system.name" => "redis",
        "db.operation.name" => operation,
        "error.type" => status
    )
    .record(duration_ms / 1000.0);
}

pub fn set_users_total(count: i64) {
    if !api_metrics_enabled() {
        return;
    }
    metrics::gauge!("immich.users.total").set(count as f64);
}

pub fn add_users_total(delta: i64) {
    if !api_metrics_enabled() {
        return;
    }
    if delta >= 0 {
        metrics::gauge!("immich.users.total").increment(delta as f64);
    } else {
        metrics::gauge!("immich.users.total").decrement((-delta) as f64);
    }
}

fn sanitize_metric_name(value: &str) -> String {
    value.replace('.', "_").replace('-', "_")
}

/// lodash `snakeCase` for ASCII identifiers used as queue and job names.
fn snake_case(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    for (index, &ch) in chars.iter().enumerate() {
        if ch == '-' || ch == ' ' || ch == '.' {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            continue;
        }
        if ch.is_ascii_uppercase() {
            let prev_lower = index > 0 && chars[index - 1].is_ascii_lowercase();
            let prev_upper = index > 0 && chars[index - 1].is_ascii_uppercase();
            let next_lower = chars
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_lowercase());
            if index > 0 && (prev_lower || (prev_upper && next_lower)) && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn metric_name(name: &str) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(existing) = guard.get(name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    guard.insert(name.to_string(), leaked);
    leaked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_include_all_enables_repo_and_io() {
        let env = EnvDto {
            immich_telemetry_include: Some("all".into()),
            ..EnvDto::default()
        };
        let config = parse_telemetry(&env);
        assert!(config.metrics.contains(&ImmichTelemetry::Repo));
        assert!(config.metrics.contains(&ImmichTelemetry::Io));
    }

    #[test]
    fn parse_include_list_and_exclude() {
        let env = EnvDto {
            immich_telemetry_include: Some("api,repo,io".into()),
            immich_telemetry_exclude: Some("io".into()),
            ..EnvDto::default()
        };
        let config = parse_telemetry(&env);
        assert!(config.metrics.contains(&ImmichTelemetry::Api));
        assert!(config.metrics.contains(&ImmichTelemetry::Repo));
        assert!(!config.metrics.contains(&ImmichTelemetry::Io));
    }

    #[test]
    fn snake_case_matches_queue_and_job_names() {
        assert_eq!(snake_case("thumbnailGeneration"), "thumbnail_generation");
        assert_eq!(snake_case("notifications"), "notifications");
        assert_eq!(
            snake_case("AssetDetectFacesQueueAll"),
            "asset_detect_faces_queue_all"
        );
        assert_eq!(snake_case("ocr"), "ocr");
    }
}
