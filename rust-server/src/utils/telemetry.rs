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
            eprintln!("prometheus metrics exporter failed on port {port}: {err}");
        }
    });
}

pub fn record_http_request(duration_ms: f64, status: u16) {
    if !api_metrics_enabled() {
        return;
    }
    metrics::counter!("immich.http.requests.total").increment(1);
    metrics::histogram!("immich.http.request.duration_ms").record(duration_ms);
    metrics::counter!("immich.http.responses.total", "status" => status_group(status))
        .increment(1);
}

pub fn record_job_finished(queue: &str, job_name: &str, success: bool) {
    if !job_metrics_enabled() {
        return;
    }
    let queue = sanitize_metric_name(queue);
    let job_name = sanitize_metric_name(job_name);
    metrics::counter!("immich.queues.started", "queue" => queue.clone()).increment(1);
    if success {
        metrics::counter!("immich.queues.completed", "queue" => queue, "job" => job_name)
            .increment(1);
    } else {
        metrics::counter!("immich.queues.failed", "queue" => queue, "job" => job_name)
            .increment(1);
    }
}

pub fn record_job_started(queue: &str, job_name: &str) {
    if !job_metrics_enabled() {
        return;
    }
    let queue = sanitize_metric_name(queue);
    let job_name = sanitize_metric_name(job_name);
    metrics::counter!(
        "immich.queues.jobs.started",
        "queue" => queue,
        "job" => job_name
    )
    .increment(1);
}

pub fn record_queue_active_delta(queue: &str, delta: i64) {
    if !job_metrics_enabled() {
        return;
    }
    let queue = sanitize_metric_name(queue);
    if delta >= 0 {
        metrics::gauge!("immich.queues.active", "queue" => queue).increment(delta as f64);
    } else {
        metrics::gauge!("immich.queues.active", "queue" => queue)
            .decrement((-delta) as f64);
    }
}

pub fn record_job_status(job_name: &str, status: &str) {
    if !job_metrics_enabled() {
        return;
    }
    let job_name = sanitize_metric_name(job_name);
    let status = sanitize_metric_name(status);
    metrics::counter!("immich.jobs", "job" => job_name, "status" => status).increment(1);
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

fn status_group(status: u16) -> String {
    format!("{}xx", status / 100)
}

fn sanitize_metric_name(value: &str) -> String {
    value.replace('.', "_").replace('-', "_")
}
