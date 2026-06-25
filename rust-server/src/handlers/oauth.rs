use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Response, StatusCode};
use axum::{Extension, Json};

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::request::auth::LoginReq;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::service::oauth::{OAuthCallbackReq, OAuthConfigReq};
use crate::utils::cookie::ImmichCookie;
use crate::utils::headers::get_cookie_value;
use crate::utils::response::{
    clear_cookie, respond_with_auth_cookies, respond_with_oauth_state_cookies,
};

pub async fn mobile_redirect_handler(
    uri: axum::http::Uri,
) -> Result<(StatusCode, HeaderMap, ()), ErrorResp> {
    let url = crate::service::oauth::OAuthService::mobile_redirect(uri.to_string().as_str());
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::LOCATION,
        url.parse().map_err(|_| ErrorResp::ServerError("Invalid redirect".to_string()))?,
    );
    Ok((StatusCode::TEMPORARY_REDIRECT, headers, ()))
}

pub async fn authorize_handler(
    State(state): State<AppState>,
    Extension(login_details): Extension<LoginReq>,
    Json(dto): Json<OAuthConfigReq>,
) -> Result<Response<Body>, ErrorResp> {
    let resp = state.services.oauth.authorize(&dto).await?;
    Ok(respond_with_oauth_state_cookies(
        &serde_json::json!({ "url": resp.url }),
        login_details.is_secure,
        &resp.state,
        resp.code_verifier.as_deref(),
    ))
}

pub async fn callback_handler(
    State(state): State<AppState>,
    Extension(login_details): Extension<LoginReq>,
    headers: HeaderMap,
    Json(mut dto): Json<OAuthCallbackReq>,
) -> Result<Response<Body>, ErrorResp> {
    if dto.state.is_none() {
        dto.state = get_cookie_value(&headers, ImmichCookie::OAuthState);
    }
    if dto.code_verifier.is_none() {
        dto.code_verifier = get_cookie_value(&headers, ImmichCookie::OAuthCodeVerifier);
    }

    let body = state.services.oauth.callback(&dto, &login_details).await?;
    let mut response = respond_with_auth_cookies(
        &body,
        login_details.is_secure,
        &body.access_token,
        "oauth",
    );
    clear_oauth_cookies(&mut response);
    Ok(response)
}

pub async fn link_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    headers: HeaderMap,
    Json(mut dto): Json<OAuthCallbackReq>,
) -> Result<Json<UserAdminResponse>, ErrorResp> {
    if dto.state.is_none() {
        dto.state = get_cookie_value(&headers, ImmichCookie::OAuthState);
    }
    if dto.code_verifier.is_none() {
        dto.code_verifier = get_cookie_value(&headers, ImmichCookie::OAuthCodeVerifier);
    }
    Ok(Json(state.services.oauth.link(&auth, &dto).await?))
}

pub async fn unlink_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<UserAdminResponse>, ErrorResp> {
    Ok(Json(state.services.oauth.unlink(&auth).await?))
}

pub async fn backchannel_logout_handler(
    State(state): State<AppState>,
    axum::Form(dto): axum::Form<std::collections::HashMap<String, String>>,
) -> Result<StatusCode, ErrorResp> {
    let token = dto
        .get("logout_token")
        .ok_or_else(|| ErrorResp::BadRequest("Invalid logout token".to_string()))?;
    state.services.oauth.backchannel_logout(token).await?;
    Ok(StatusCode::OK)
}

fn clear_oauth_cookies(response: &mut Response<Body>) {
    for cookie in [ImmichCookie::OAuthState, ImmichCookie::OAuthCodeVerifier] {
        response.headers_mut().append(
            axum::http::header::SET_COOKIE,
            clear_cookie(cookie.as_str()).parse().unwrap(),
        );
    }
}
