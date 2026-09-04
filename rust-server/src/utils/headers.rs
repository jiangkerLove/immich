use axum::http::{HeaderMap, header};
use std::collections::HashMap;

use crate::utils::cookie::{ImmichCookie, parse_immich_cookies};

pub mod immich {
    pub const API_KEY: &str = "x-api-key";
    pub const USER_TOKEN: &str = "x-immich-user-token";
    pub const SESSION_TOKEN: &str = "x-immich-session-token";
    pub const SHARED_LINK_KEY: &str = "x-immich-share-key";
    pub const SHARED_LINK_SLUG: &str = "x-immich-share-slug";
}

pub mod query {
    pub const SHARED_LINK_KEY: &str = "key";
    pub const SHARED_LINK_SLUG: &str = "slug";
    pub const API_KEY: &str = "apiKey";
    pub const SESSION_KEY: &str = "sessionKey";
}

pub fn parse_query_params(uri: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    let Some(query) = uri.split('?').nth(1) else {
        return params;
    };

    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        if let (Some(key), Some(value)) = (kv.next(), kv.next()) {
            params.insert(key.to_string(), value.to_string());
        }
    }

    params
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

pub fn get_bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth = header_value(headers, header::AUTHORIZATION.as_str())?;
    let mut parts = auth.splitn(2, ' ');
    let token_type = parts.next()?;
    let token = parts.next()?;
    if token_type.eq_ignore_ascii_case("bearer") {
        Some(token.to_string())
    } else {
        None
    }
}

pub fn get_cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    let cookies = parse_immich_cookies(cookie_header);
    cookies.get(&ImmichCookie::AccessToken).cloned()
}

pub fn get_cookie_value(headers: &HeaderMap, cookie: ImmichCookie) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    let cookies = parse_immich_cookies(cookie_header);
    cookies.get(&cookie).cloned()
}

pub fn get_shared_link_tokens(headers: &HeaderMap) -> Vec<String> {
    get_cookie_value(headers, ImmichCookie::SharedLinkToken)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn get_auth_type(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    let cookies = parse_immich_cookies(cookie_header);
    cookies.get(&ImmichCookie::AuthType).cloned()
}

pub struct AuthTokens {
    pub share_key: Option<String>,
    pub share_slug: Option<String>,
    pub session: Option<String>,
    pub api_key: Option<String>,
}

pub fn extract_auth_tokens(
    headers: &HeaderMap,
    query_params: &HashMap<String, String>,
) -> AuthTokens {
    let share_key = header_value(headers, immich::SHARED_LINK_KEY)
        .or_else(|| query_params.get(query::SHARED_LINK_KEY).cloned());
    let share_slug = header_value(headers, immich::SHARED_LINK_SLUG)
        .or_else(|| query_params.get(query::SHARED_LINK_SLUG).cloned());

    let session = header_value(headers, immich::USER_TOKEN)
        .or_else(|| header_value(headers, immich::SESSION_TOKEN))
        .or_else(|| query_params.get(query::SESSION_KEY).cloned())
        .or_else(|| get_bearer_token(headers))
        .or_else(|| get_cookie_token(headers));

    let api_key = header_value(headers, immich::API_KEY)
        .or_else(|| query_params.get(query::API_KEY).cloned());

    AuthTokens {
        share_key,
        share_slug,
        session,
        api_key,
    }
}
