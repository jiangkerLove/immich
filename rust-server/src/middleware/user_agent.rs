use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::OnceLock;

use axum::extract::{ConnectInfo, Request};
use axum::middleware::Next;
use axum::response::Response;
use user_agent_parser::UserAgentParser;

use crate::models::request::auth::LoginReq;

static UA_PARSER: OnceLock<UserAgentParser> = OnceLock::new();

fn ua_parser() -> &'static UserAgentParser {
    UA_PARSER.get_or_init(|| UserAgentParser::from_path("regexes.yaml").unwrap())
}

pub async fn user_agent(mut req: Request, next: Next) -> Response {
    if req.uri().path() == "/api/auth/login" {
        let login_details = parse_user_agent(&req);
        req.extensions_mut().insert(login_details);
    }

    next.run(req).await
}

fn parse_user_agent(req: &Request) -> LoginReq {
    let addr_ip = if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
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

    let user_agent = req
        .headers()
        .get(axum::http::header::USER_AGENT)
        .and_then(|header| header.to_str().ok())
        .unwrap_or("");

    let parser = ua_parser();
    let os_str = parser.parse_os(user_agent).name.unwrap_or(Cow::from(""));
    let device_type = parser
        .parse_product(user_agent)
        .name
        .unwrap_or(Cow::from(""));

    LoginReq {
        client_ip,
        is_secure,
        device_type: device_type.to_string(),
        device_os: os_str.to_string(),
    }
}
