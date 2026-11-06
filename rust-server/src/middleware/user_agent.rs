use std::borrow::Cow;
use std::net::{IpAddr, SocketAddr};
use std::sync::OnceLock;

use axum::extract::{ConnectInfo, Request};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use user_agent_parser::UserAgentParser;

use crate::models::request::auth::LoginReq;

static UA_PARSER: OnceLock<UserAgentParser> = OnceLock::new();
static CORRELATION_HEADER: HeaderName = HeaderName::from_static("x-correlation-id");

fn ua_parser() -> &'static UserAgentParser {
    UA_PARSER.get_or_init(|| {
        UserAgentParser::from_str(include_str!("../../regexes.yaml"))
            .expect("embedded user-agent regexes")
    })
}

pub async fn user_agent(mut req: Request, next: Next) -> Response {
    let login_details = parse_user_agent(&req);
    req.extensions_mut().insert(login_details);

    let correlation_id = req
        .headers()
        .get(&CORRELATION_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut response = next.run(req).await;
    if !response.headers().contains_key(&CORRELATION_HEADER) {
        if let Ok(value) = HeaderValue::from_str(&correlation_id) {
            response
                .headers_mut()
                .insert(CORRELATION_HEADER.clone(), value);
        }
    }
    response
}

pub fn app_version_from_ua(user_agent: &str) -> Option<String> {
    const PREFIXES: &[&str] = &[
        "immich-android/",
        "immich-ios/",
        "immich-unknown/",
        "Immich_Android_",
        "Immich_iOS_",
        "Immich_Unknown_",
    ];
    for prefix in PREFIXES {
        if let Some(version) = user_agent.strip_prefix(prefix) {
            if !version.is_empty() {
                return Some(version.to_string());
            }
        }
    }
    None
}

pub fn login_details_from_headers(headers: &HeaderMap) -> LoginReq {
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|header| header.to_str().ok())
        .unwrap_or("");
    let parser = ua_parser();
    let os_str = parser.parse_os(user_agent).name.unwrap_or(Cow::from(""));
    let browser = parser
        .parse_product(user_agent)
        .name
        .unwrap_or(Cow::from(""));
    // ua-parser-js `device.type` (mobile/tablet/…). Approximate from UA tokens
    // when the product/browser name is empty.
    let device_kind = device_type_from_ua(user_agent);
    let device_header = headers
        .get("devicetype")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let model_header = headers
        .get("devicemodel")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    let device_type = if !browser.is_empty() {
        browser.to_string()
    } else if !device_kind.is_empty() {
        device_kind.to_string()
    } else {
        model_header.to_string()
    };

    LoginReq {
        client_ip: String::new(),
        is_secure: false,
        device_type,
        device_os: if os_str.is_empty() {
            device_header.to_string()
        } else {
            os_str.to_string()
        },
        app_version: app_version_from_ua(user_agent),
    }
}

fn device_type_from_ua(user_agent: &str) -> &'static str {
    // Immich app UAs (`immich-android/…`) should fall through to `devicemodel`.
    if app_version_from_ua(user_agent).is_some() {
        return "";
    }
    let lower = user_agent.to_ascii_lowercase();
    if lower.contains("ipad")
        || lower.contains("tablet")
        || lower.contains("kindle")
        || lower.contains("silk")
    {
        "tablet"
    } else if lower.contains("mobi")
        || lower.contains("iphone")
        || lower.contains("ipod")
        || (lower.contains("android") && !lower.contains("tablet"))
    {
        "mobile"
    } else {
        ""
    }
}

fn parse_user_agent(req: &Request) -> LoginReq {
    let peer_ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip())
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok())
        });

    let client_ip = resolve_client_ip(
        peer_ip,
        req.headers()
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok()),
        trusted_proxy_tokens(),
    );

    let is_secure = req.uri().scheme_str() == Some("https")
        || req
            .headers()
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == "https");

    let parsed = login_details_from_headers(req.headers());
    LoginReq {
        client_ip,
        is_secure,
        device_type: parsed.device_type,
        device_os: parsed.device_os,
        app_version: parsed.app_version,
    }
}

fn trusted_proxy_tokens() -> Vec<String> {
    crate::service::config_bootstrap::trusted_proxies()
}

/// Express `trust proxy` + `req.ip`: walk X-Forwarded-For from the right,
/// skip trusted hops, return the first untrusted address.
fn resolve_client_ip(
    peer: Option<IpAddr>,
    forwarded_for: Option<&str>,
    trusted_tokens: Vec<String>,
) -> String {
    let peer = peer.unwrap_or_else(|| IpAddr::from([0, 0, 0, 0]));
    if !is_trusted_addr(peer, &trusted_tokens) {
        return peer.to_string();
    }

    let mut hops: Vec<IpAddr> = forwarded_for
        .unwrap_or("")
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    if hops.is_empty() {
        return peer.to_string();
    }

    // proxy-addr: scan from the right, skip trusted, return first untrusted.
    while let Some(addr) = hops.pop() {
        if !is_trusted_addr(addr, &trusted_tokens) {
            return addr.to_string();
        }
    }

    // All hops trusted — leftmost original client (Express falls back similarly).
    let peer_string = peer.to_string();
    forwarded_for
        .unwrap_or("")
        .split(',')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(peer_string.as_str())
        .to_string()
}

fn is_trusted_addr(addr: IpAddr, tokens: &[String]) -> bool {
    // `app.common.ts` always includes `loopback` in `trust proxy`.
    if addr.is_loopback() {
        return true;
    }
    tokens.iter().any(|token| match_trust_token(addr, token))
}

fn match_trust_token(addr: IpAddr, token: &str) -> bool {
    match token {
        "loopback" => addr.is_loopback(),
        "linklocal" => match addr {
            IpAddr::V4(v4) => v4.is_link_local(),
            IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
        },
        "uniquelocal" => match addr {
            IpAddr::V4(v4) => v4.is_private(),
            IpAddr::V6(v6) => (v6.segments()[0] & 0xfe00) == 0xfc00,
        },
        other => other
            .parse::<IpAddr>()
            .map(|trusted| trusted == addr)
            .unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::{device_type_from_ua, resolve_client_ip};
    use std::net::IpAddr;

    #[test]
    fn device_type_falls_back_to_mobile_or_tablet() {
        assert_eq!(
            device_type_from_ua(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15"
            ),
            "mobile"
        );
        assert_eq!(
            device_type_from_ua("Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605"),
            "tablet"
        );
        assert_eq!(device_type_from_ua("Immich_Android_1.0"), "");
    }

    #[test]
    fn client_ip_ignores_xff_from_untrusted_peer() {
        let peer: IpAddr = "203.0.113.9".parse().unwrap();
        let ip = resolve_client_ip(
            Some(peer),
            Some("198.51.100.1, 10.0.0.1"),
            vec!["linklocal".into(), "uniquelocal".into()],
        );
        assert_eq!(ip, "203.0.113.9");
    }

    #[test]
    fn client_ip_uses_xff_when_peer_is_private() {
        let peer: IpAddr = "10.0.0.2".parse().unwrap();
        let ip = resolve_client_ip(
            Some(peer),
            Some("198.51.100.7, 10.0.0.2"),
            vec!["linklocal".into(), "uniquelocal".into()],
        );
        assert_eq!(ip, "198.51.100.7");
    }
}
