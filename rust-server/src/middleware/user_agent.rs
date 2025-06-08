use std::borrow::Cow;
use std::net::SocketAddr;
use axum::extract::{ConnectInfo, Request};
use axum::http;
use axum::http::{StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use user_agent_parser::UserAgentParser;
use crate::dtos::auth::LoginDetails;

pub async fn user_agent(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let path = req.uri().path();

    match path {
        "/api/auth/login" => {
            let login_details = paras_user_agent(&req);
            req.extensions_mut().insert(login_details);
            Ok(next.run(req).await)
        }
        _ => {
            Ok(next.run(req).await)
        }
    }
}

fn paras_user_agent(req: &Request) -> LoginDetails {
    let addr_ip = if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        addr.ip().to_string()
    } else {
        if let Some(real_ip) = req.headers().get("x-real-ip") {
            if let Ok(real_ip_str) = real_ip.to_str() {
                real_ip_str.to_string()
            } else {
                "unknown".to_string()
            }
        } else {
            "unknown".to_string()
        }
    };

    // 获取 IP（优先 X-Forwarded-For）
    let client_ip = req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(addr_ip.as_str())
        .to_string();


    let is_secure = req.uri().scheme_str() == Some("https");

    let user_agent = req.headers()
        .get(http::header::USER_AGENT)
        .and_then(|header| header.to_str().ok()).unwrap();

    let ua_parser = UserAgentParser::from_path("regexes.yaml").unwrap();
    let os_str = ua_parser.parse_os(user_agent).name.unwrap_or(Cow::from(""));
    let device_type = ua_parser.parse_product(user_agent).name.unwrap_or(Cow::from(""));
    LoginDetails {
        client_ip,
        is_secure,
        device_type: device_type.to_string(),
        device_os: os_str.to_string(),
    }
}