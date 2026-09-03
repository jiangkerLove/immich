use chrono::Utc;
use reqwest::Url;
use semver::{Version, VersionReq};
use serde::Deserialize;
use sqlx::PgPool;

use crate::constants::SERVER_VERSION;
use crate::models::db::system_metadata::{self, VersionCheckState};
use crate::models::dto::env::ImmichEnvironment;
use crate::service::server::ServerService;
use crate::service::websocket::WebSocketHub;

const VERSION_CHECK_URL_PROD: &str = "https://version.immich.cloud/version";
const VERSION_CHECK_URL_DEV: &str = "https://version.dev.immich.cloud/version";
const MIN_CHECK_INTERVAL_SECS: i64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionCheckOutcome {
    Success,
    Skipped,
}

#[derive(Debug, Deserialize)]
struct VersionResponse {
    version: String,
    published_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseEvent {
    pub is_available: bool,
    pub checked_at: String,
    pub server_version: crate::service::server::ServerVersionResponse,
    pub release_version: crate::service::server::ServerVersionResponse,
    #[serde(rename = "type")]
    pub release_type: Option<String>,
}

pub fn build_release_event(channel: &str, metadata: &VersionCheckState) -> Option<ReleaseEvent> {
    let release_version = metadata.release_version.as_deref()?;
    let checked_at = metadata.checked_at.clone().unwrap_or_default();
    let include_prerelease = channel == "releaseCandidate";
    Some(ReleaseEvent {
        is_available: is_newer_release(SERVER_VERSION, release_version, include_prerelease),
        checked_at,
        server_version: ServerService::version(),
        release_version: parse_server_version(release_version),
        release_type: diff_release_type(SERVER_VERSION, release_version),
    })
}

pub async fn on_websocket_connect(
    pool: &PgPool,
    emit: impl FnOnce(&'static str, ReleaseEvent) + Send,
) -> Result<(), String> {
    let config = crate::utils::system_config::get_merged(pool)
        .await
        .map_err(|err| err.to_string())?;
    let version_check = config.get("newVersionCheck");
    let enabled = version_check
        .and_then(|value| value.get("enabled"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    if !enabled {
        return Ok(());
    }

    let channel = version_check
        .and_then(|value| value.get("channel"))
        .and_then(|value| value.as_str())
        .unwrap_or("stable");
    let metadata = system_metadata::get_version_check_state(pool)
        .await
        .map_err(|err| err.to_string())?;
    if let Some(event) = build_release_event(channel, &metadata) {
        emit("on_new_release", event);
    }

    Ok(())
}

pub fn should_skip_version_check(enabled: bool, seconds_since_last_check: Option<i64>) -> bool {
    if !enabled {
        return true;
    }
    matches!(
        seconds_since_last_check,
        Some(secs) if secs < MIN_CHECK_INTERVAL_SECS
    )
}

pub async fn run_version_check(
    pool: &PgPool,
    websocket: &WebSocketHub,
    env: Option<&ImmichEnvironment>,
) -> Result<VersionCheckOutcome, String> {
    let config = crate::utils::system_config::get_merged(pool)
        .await
        .map_err(|err| err.to_string())?;

    let version_check = config.get("newVersionCheck");
    let enabled = version_check
        .and_then(|value| value.get("enabled"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    if !enabled {
        return Ok(VersionCheckOutcome::Skipped);
    }

    let channel = version_check
        .and_then(|value| value.get("channel"))
        .and_then(|value| value.as_str())
        .unwrap_or("stable");
    let include_prerelease = channel == "releaseCandidate";

    let existing = system_metadata::get_version_check_state(pool)
        .await
        .map_err(|err| err.to_string())?;
    let seconds_since_last_check = existing.checked_at.as_deref().and_then(|checked_at| {
        chrono::DateTime::parse_from_rfc3339(checked_at)
            .ok()
            .map(|parsed| {
                Utc::now()
                    .signed_duration_since(parsed.with_timezone(&Utc))
                    .num_seconds()
            })
    });
    if should_skip_version_check(true, seconds_since_last_check) {
        return Ok(VersionCheckOutcome::Skipped);
    }

    let release = fetch_latest_release(env, channel).await?;
    let checked_at = Utc::now().to_rfc3339();
    let metadata = VersionCheckState {
        checked_at: Some(checked_at.clone()),
        release_version: Some(release.version.clone()),
    };
    system_metadata::set_json(
        pool,
        "version-check-state",
        &serde_json::to_value(&metadata).unwrap_or_default(),
    )
    .await
    .map_err(|err| err.to_string())?;

    if is_newer_release(SERVER_VERSION, &release.version, include_prerelease) {
        println!(
            "version check: found {} released at {}",
            release.version, release.published_at
        );
        if let Some(payload) = build_release_event(
            channel,
            &VersionCheckState {
                checked_at: Some(checked_at),
                release_version: Some(release.version.clone()),
            },
        ) {
            websocket.client_broadcast("on_new_release", payload);
        }
    }

    Ok(VersionCheckOutcome::Success)
}

async fn fetch_latest_release(
    env: Option<&ImmichEnvironment>,
    channel: &str,
) -> Result<VersionResponse, String> {
    let base = match env {
        Some(ImmichEnvironment::Development) | Some(ImmichEnvironment::Test) => {
            VERSION_CHECK_URL_DEV
        }
        _ => VERSION_CHECK_URL_PROD,
    };

    let mut url = Url::parse(base).map_err(|err| err.to_string())?;
    url.query_pairs_mut().append_pair(
        "channel",
        if channel == "releaseCandidate" {
            "rc"
        } else {
            "stable"
        },
    );

    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "version check request failed with status {}",
            response.status()
        ));
    }

    response
        .json::<VersionResponse>()
        .await
        .map_err(|err| err.to_string())
}

fn normalize_version(value: &str) -> String {
    value.trim().trim_start_matches('v').to_string()
}

fn parse_version(value: &str) -> Option<Version> {
    Version::parse(&normalize_version(value)).ok()
}

fn is_newer_release(current: &str, release: &str, include_prerelease: bool) -> bool {
    let Some(current_v) = parse_version(current) else {
        return false;
    };
    let Some(release_v) = parse_version(release) else {
        return false;
    };

    if release_v <= current_v {
        return false;
    }

    if include_prerelease {
        return true;
    }

    if !release_v.pre.is_empty() {
        return false;
    }

    let Ok(req) = VersionReq::parse(&format!(">{current_v}")) else {
        return false;
    };

    req.matches(&release_v)
}

fn parse_server_version(value: &str) -> crate::service::server::ServerVersionResponse {
    let version = parse_version(value).unwrap_or_else(|| Version::new(0, 0, 0));
    let prerelease = version
        .pre
        .as_str()
        .rsplit('.')
        .next()
        .and_then(|part| part.parse::<u64>().ok());
    crate::service::server::ServerVersionResponse {
        major: version.major,
        minor: version.minor,
        patch: version.patch,
        prerelease,
    }
}

fn diff_release_type(current: &str, release: &str) -> Option<String> {
    let current_v = parse_version(current)?;
    let release_v = parse_version(release)?;
    if release_v.major > current_v.major {
        return Some("major".to_string());
    }
    if release_v.minor > current_v.minor {
        return Some("minor".to_string());
    }
    if release_v.patch > current_v.patch {
        return Some("patch".to_string());
    }
    if !release_v.pre.is_empty() {
        return Some("prerelease".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_when_disabled() {
        assert!(should_skip_version_check(false, None));
        assert!(should_skip_version_check(false, Some(100)));
    }

    #[test]
    fn skips_when_checked_within_fifty_seconds() {
        assert!(should_skip_version_check(true, Some(0)));
        assert!(should_skip_version_check(true, Some(49)));
        assert!(!should_skip_version_check(true, Some(50)));
        assert!(!should_skip_version_check(true, None));
    }

    #[test]
    fn build_release_event_marks_older_release_unavailable() {
        let metadata = VersionCheckState {
            checked_at: Some("2024-01-01T00:00:00Z".to_string()),
            release_version: Some("0.0.1".to_string()),
        };
        let event = build_release_event("stable", &metadata).expect("event");
        assert!(!event.is_available);
    }

    #[test]
    fn build_release_event_requires_release_version() {
        let metadata = VersionCheckState {
            checked_at: Some("2024-01-01T00:00:00Z".to_string()),
            release_version: None,
        };
        assert!(build_release_event("stable", &metadata).is_none());
    }

    #[test]
    fn newer_release_uses_version_requirement() {
        assert!(is_newer_release("1.0.0", "1.1.0", false));
        assert!(!is_newer_release("1.0.0", "1.0.0", false));
        assert!(!is_newer_release("1.0.0", "1.1.0-rc.1", false));
        assert!(is_newer_release("1.0.0", "1.1.0-rc.1", true));
    }
}
