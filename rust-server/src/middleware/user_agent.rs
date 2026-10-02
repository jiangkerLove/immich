use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::OnceLock;

use axum::extract::{ConnectInfo, Request};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use user_agent_parser::UserAgentParser;

use crate::models::request::auth::LoginReq;

static UA_PARSER: OnceLock<UserAgentParser> = OnceLock::new();

fn ua_parser() -> &'static UserAgentParser {
    UA_PARSER.get_or_init(|| {
        UserAgentParser::from_str(include_str!("../../regexes.yaml"))
            .expect("embedded user-agent regexes")
    })
}

pub async fn user_agent(mut req: Request, next: Next) -> Response {
    let login_details = parse_user_agent(&req);
    req.extensions_mut().insert(login_details);
    next.run(req).await
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
    let device_type = parser
        .parse_product(user_agent)
        .name
        .unwrap_or(Cow::from(""));
    let device_header = headers
        .get("devicetype")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let model_header = headers
        .get("devicemodel")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    LoginReq {
        client_ip: String::new(),
        is_secure: false,
        device_type: if device_type.is_empty() {
            model_header.to_string()
        } else {
            device_type.to_string()
        },
        device_os: if os_str.is_empty() {
            device_header.to_string()
        } else {
            os_str.to_string()
        },
        app_version: app_version_from_ua(user_agent),
    }
}

fn parse_user_agent(req: &Request) -> LoginReq {
    let addr_ip = if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>()
    {
        addr.ip().to_string()
    } else {
        req.headers()
            .get("x-real-ip")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("unknown")
            .to_string()
    };

    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .unwrap_or(addr_ip.as_str())
        .to_string();

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
