use std::sync::OnceLock;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::Registry;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::reload;
use tracing_subscriber::util::SubscriberInitExt;

static RELOAD_HANDLE: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

/// Initialize tracing. Prefer `RUST_LOG` when set; otherwise default to INFO.
/// Call early from bootstrap before heavy work. Level can be changed later via
/// [`apply_level`] once Immich config / `IMMICH_LOG_LEVEL` is known.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let (filter_layer, handle) = reload::Layer::new(filter);
    let _ = RELOAD_HANDLE.set(handle);

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt::layer().with_target(true))
        .init();
}

/// Map Immich log levels onto tracing filters and apply at runtime.
pub fn apply_level(level: &str) {
    let directive = map_immich_level(level);
    let Some(handle) = RELOAD_HANDLE.get() else {
        return;
    };
    if let Err(err) = handle.reload(EnvFilter::new(directive)) {
        eprintln!("logging: failed to apply level {directive}: {err}");
    }
}

fn map_immich_level(level: &str) -> &'static str {
    match level.trim().to_ascii_lowercase().as_str() {
        "false" | "off" => "off",
        "verbose" | "trace" => "trace",
        "debug" => "debug",
        "log" | "info" => "info",
        "warn" => "warn",
        "error" | "fatal" => "error",
        _ => "info",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_immich_levels() {
        assert_eq!(map_immich_level("false"), "off");
        assert_eq!(map_immich_level("off"), "off");
        assert_eq!(map_immich_level("OFF"), "off");
        assert_eq!(map_immich_level("verbose"), "trace");
        assert_eq!(map_immich_level("trace"), "trace");
        assert_eq!(map_immich_level("debug"), "debug");
        assert_eq!(map_immich_level("log"), "info");
        assert_eq!(map_immich_level("info"), "info");
        assert_eq!(map_immich_level("warn"), "warn");
        assert_eq!(map_immich_level("error"), "error");
        assert_eq!(map_immich_level("fatal"), "error");
        assert_eq!(map_immich_level("unknown"), "info");
        assert_eq!(map_immich_level("  Debug  "), "debug");
    }
}
