use axum::extract::State;
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

    match app_state
        .services
        .auth
        .authenticate(&tokens, &path, &shared_link_tokens)
        .await {
        Ok(auth) => {
            req.extensions_mut().insert(auth);
            next.run(req).await
        }
        Err(err) => handler_err(err),
    }
}
