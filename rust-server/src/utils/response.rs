use axum::body::Body;
use axum::http::{header, HeaderValue, Response, StatusCode};
use axum::response::IntoResponse;
use serde::Serialize;

use crate::utils::cookie::ImmichCookie;

const COOKIE_MAX_AGE_SECS: i64 = 400 * 24 * 60 * 60;

pub fn json_response<T: Serialize>(value: &T) -> Response<Body> {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_string(value).unwrap()))
        .unwrap()
}

pub fn json_response_with_status<T: Serialize>(status: StatusCode, value: &T) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_string(value).unwrap()))
        .unwrap()
}

fn build_cookie(name: &str, value: &str, is_secure: bool, http_only: bool) -> String {
    let mut cookie = format!(
        "{name}={value}; Path=/; SameSite=Lax; Max-Age={COOKIE_MAX_AGE_SECS}"
    );
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if is_secure {
        cookie.push_str("; Secure");
    }
    cookie
}

pub fn clear_cookie(name: &str) -> String {
    format!("{name}=; Path=/; Max-Age=0; HttpOnly")
}

pub fn respond_with_oauth_state_cookies<T: Serialize>(
    body: &T,
    is_secure: bool,
    state: &str,
    code_verifier: Option<&str>,
) -> Response<Body> {
    let mut response = json_response(body);
    let headers = response.headers_mut();
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_cookie(
            ImmichCookie::OAuthState.as_str(),
            state,
            is_secure,
            true,
        ))
        .unwrap(),
    );
    if let Some(verifier) = code_verifier {
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_str(&build_cookie(
                ImmichCookie::OAuthCodeVerifier.as_str(),
                verifier,
                is_secure,
                true,
            ))
            .unwrap(),
        );
    }
    response
}

pub fn respond_with_auth_cookies<T: Serialize>(
    body: &T,
    is_secure: bool,
    access_token: &str,
    auth_type: &str,
) -> Response<Body> {
    let mut response = json_response(body);
    let headers = response.headers_mut();
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_cookie(
            ImmichCookie::AccessToken.as_str(),
            access_token,
            is_secure,
            true,
        ))
        .unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_cookie(
            ImmichCookie::AuthType.as_str(),
            auth_type,
            is_secure,
            true,
        ))
        .unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_cookie(
            ImmichCookie::IsAuthenticated.as_str(),
            "true",
            is_secure,
            false,
        ))
        .unwrap(),
    );
    response
}

pub fn respond_with_shared_link_cookie<T: Serialize>(
    body: &T,
    is_secure: bool,
    token_value: &str,
) -> Response<Body> {
    let mut response = json_response(body);
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_cookie(
            ImmichCookie::SharedLinkToken.as_str(),
            token_value,
            is_secure,
            true,
        ))
        .unwrap(),
    );
    response
}

pub fn respond_without_auth_cookies<T: Serialize>(body: &T) -> Response<Body> {
    let mut response = json_response(body);
    let headers = response.headers_mut();
    for cookie in [
        ImmichCookie::AccessToken,
        ImmichCookie::AuthType,
        ImmichCookie::IsAuthenticated,
    ] {
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_str(&clear_cookie(cookie.as_str())).unwrap(),
        );
    }
    response
}

pub struct JsonBody<T>(pub T);

impl<T: Serialize> IntoResponse for JsonBody<T> {
    fn into_response(self) -> Response<Body> {
        json_response(&self.0)
    }
}
