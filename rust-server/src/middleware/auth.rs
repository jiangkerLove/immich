use axum::extract::State;
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;

use crate::app_state::AppState;
use crate::models::response::response::handler_err;
use crate::utils::headers::{extract_auth_tokens, get_shared_link_tokens, parse_query_params};

const PUBLIC_ROUTES: &[&str] = &[
    "/api/auth/login",
    "/api/auth/admin-sign-up",
    "/api/oauth/mobile-redirect",
    "/api/oauth/authorize",
    "/api/oauth/callback",
    "/api/oauth/backchannel-logout",
    "/api/server/ping",
    "/api/server/version",
    "/api/server/version-history",
    "/api/server/features",
    "/api/server/config",
    "/api/public/config",
    "/api/public/config/defaults",
    "/api/server/media-types",
    "/api/admin/maintenance/status",
    "/api/admin/maintenance/login",
    "/api/admin/database-backups/start-restore",
    "/.well-known/immich",
    "/custom.css",
];

fn is_public_route(path: &str) -> bool {
    PUBLIC_ROUTES.contains(&path)
}

/// Routes a shared link may call. Matches `SHARED_LINK_ROUTES` in
/// `server/src/controllers/index.spec.ts`.
const SHARED_LINK_ROUTES: &[(&str, &str)] = &[
    ("DELETE", "/api/assets/:id/video/stream/:sessionId"),
    ("GET", "/api/albums/:id"),
    ("GET", "/api/albums/:id/map-markers"),
    ("GET", "/api/assets/:id"),
    ("GET", "/api/assets/:id/original"),
    ("GET", "/api/assets/:id/thumbnail"),
    ("GET", "/api/assets/:id/video/playback"),
    (
        "GET",
        "/api/assets/:id/video/stream/:sessionId/:variantIndex/:filename",
    ),
    (
        "GET",
        "/api/assets/:id/video/stream/:sessionId/:variantIndex/playlist.m3u8",
    ),
    ("GET", "/api/assets/:id/video/stream/main.m3u8"),
    ("GET", "/api/shared-links/me"),
    ("GET", "/api/timeline/bucket"),
    ("GET", "/api/timeline/buckets"),
    ("POST", "/api/assets"),
    ("POST", "/api/download/archive"),
    ("POST", "/api/download/info"),
    ("POST", "/api/search/metadata"),
    ("POST", "/api/shared-links/login"),
];

fn shared_link_route_allowed(method: &Method, path: &str) -> bool {
    let method = method.as_str();
    SHARED_LINK_ROUTES
        .iter()
        .any(|(allowed_method, pattern)| *allowed_method == method && path_matches(pattern, path))
}

fn path_matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').filter(|part| !part.is_empty()).collect();
    let path: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if pattern.len() != path.len() {
        return false;
    }
    pattern
        .iter()
        .zip(path)
        .all(|(expected, actual)| expected.starts_with(':') || *expected == actual)
}

pub async fn require_auth(
    State(app_state): State<AppState>,
    mut req: axum::extract::Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    if is_public_route(&path) {
        return next.run(req).await;
    }

    let query_params = parse_query_params(req.uri().to_string().as_str());
    let tokens = extract_auth_tokens(req.headers(), &query_params);
    let shared_link_tokens = get_shared_link_tokens(req.headers());

    let client = req
        .extensions()
        .get::<crate::models::request::auth::LoginReq>()
        .cloned()
        .unwrap_or(crate::models::request::auth::LoginReq {
            is_secure: false,
            client_ip: String::new(),
            device_type: String::new(),
            device_os: String::new(),
            app_version: None,
        });

    match app_state
        .services
        .auth
        .authenticate(&tokens, &path, &shared_link_tokens, &client)
        .await
    {
        Ok(auth) => {
            if auth.shared_link.is_some() && !shared_link_route_allowed(req.method(), &path) {
                return handler_err(crate::models::response::response::ErrorResp::Forbidden(
                    "Forbidden".to_string(),
                ));
            }
            req.extensions_mut().insert(auth);
            next.run(req).await
        }
        Err(err) => handler_err(err),
    }
}
