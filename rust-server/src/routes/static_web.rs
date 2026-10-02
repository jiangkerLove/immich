use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Router};
use tower::service_fn;
use tower_http::services::ServeDir;

use crate::app_state::AppState;
use crate::service::shared_link::OpenGraphTags;

/// `server/src/constants.ts` `excludePaths`. These stay on their own routes or files.
const SSR_EXCLUDE_PREFIXES: &[&str] = &["/.well-known/immich", "/custom.css", "/favicon.ico"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum WebMode {
    /// `server/src/services/api.service.ts` `ssr`.
    App,
    /// `server/src/maintenance/maintenance-worker.service.ts` `ssr`.
    Maintenance,
}

pub fn resolve_web_root(env: &crate::models::dto::env::EnvDto) -> Option<PathBuf> {
    if let Some(root) = env.immich_web_root.as_ref() {
        let path = PathBuf::from(root);
        if path.join("index.html").is_file() {
            return Some(path);
        }
        tracing::error!(
            "IMMICH_WEB_ROOT={} has no index.html; static web UI disabled",
            root
        );
        return None;
    }

    for candidate in default_web_roots() {
        if candidate.join("index.html").is_file() {
            return Some(candidate);
        }
    }

    None
}

fn default_web_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join("www"));
            roots.push(dir.join("../www"));
            roots.push(dir.join("../../server/build/www"));
        }
    }
    roots.push(PathBuf::from("./www"));
    roots.push(PathBuf::from("/build/www"));
    roots.push(PathBuf::from("../server/build/www"));
    roots
}

pub fn fallback_router(web_root: &Path) -> Router<AppState> {
    web_router(web_root, WebMode::App)
}

pub fn maintenance_fallback_router(web_root: &Path) -> Router<AppState> {
    web_router(web_root, WebMode::Maintenance)
}

fn web_router(web_root: &Path, mode: WebMode) -> Router<AppState> {
    let index_html =
        Arc::new(std::fs::read_to_string(web_root.join("index.html")).unwrap_or_default());
    let index_for_fallback = Arc::clone(&index_html);
    // sirv in `server/src/app.common.ts`: existing files first, including gzip and brotli.
    // `extensions: []` means `/photos` is not rewritten to `photos.html`.
    let files = ServeDir::new(web_root)
        .precompressed_gzip()
        .precompressed_br()
        .call_fallback_on_method_not_allowed(true)
        .fallback(service_fn(move |request: Request<Body>| {
            let index_html = Arc::clone(&index_for_fallback);
            async move { Ok::<_, Infallible>(spa_fallback(mode, index_html.as_ref(), &request)) }
        }));

    let mut router = Router::new();
    if mode == WebMode::App {
        router = router
            .route("/share/{*path}", get(ssr_share_key).head(ssr_share_key))
            .route("/s/{*path}", get(ssr_share_slug).head(ssr_share_slug));
    }

    router
        .fallback_service(files)
        .layer(Extension(index_html))
        .layer(middleware::from_fn(immutable_asset_cache))
}

async fn immutable_asset_cache(request: Request<Body>, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;
    let cache_control = response
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok());
    if response.status() == StatusCode::OK
        && path.starts_with("/_app/immutable")
        && cache_control != Some("no-store")
    {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public,max-age=31536000,immutable"),
        );
    }
    response
}

fn spa_fallback(mode: WebMode, index_html: &str, request: &Request<Body>) -> Response {
    let accept = request
        .headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok());
    match spa_action(
        mode,
        request.method(),
        request.uri().path(),
        &request_target(request.uri()),
        accept,
    ) {
        SpaAction::NotFound => StatusCode::NOT_FOUND.into_response(),
        SpaAction::NotAcceptable { path, accept } => not_acceptable(&path, &accept),
        SpaAction::Redirect(location) => redirect_to(&location),
        SpaAction::Html => html_response(StatusCode::OK, index_html.to_string()),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SpaAction {
    NotFound,
    NotAcceptable { path: String, accept: String },
    Redirect(String),
    Html,
}

fn spa_action(
    mode: WebMode,
    method: &Method,
    path: &str,
    target: &str,
    accept: Option<&str>,
) -> SpaAction {
    let method_allowed = match mode {
        WebMode::App => *method == Method::GET || *method == Method::HEAD,
        WebMode::Maintenance => *method == Method::GET,
    };
    if target.starts_with("/api")
        || !method_allowed
        || SSR_EXCLUDE_PREFIXES
            .iter()
            .any(|prefix| target.starts_with(prefix))
    {
        return SpaAction::NotFound;
    }

    if mode == WebMode::App && !accepts_html(accept) {
        return SpaAction::NotAcceptable {
            path: path.to_string(),
            accept: accept.unwrap_or("").to_string(),
        };
    }

    if mode == WebMode::Maintenance && !target.starts_with("/maintenance") {
        return SpaAction::Redirect(maintenance_location(path));
    }

    SpaAction::Html
}

fn accepts_html(accept: Option<&str>) -> bool {
    let Some(accept) = accept.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    accept.split(',').any(|part| {
        let media = part.split(';').next().unwrap_or("").trim();
        media.eq_ignore_ascii_case("*/*")
            || media.eq_ignore_ascii_case("text/*")
            || media.eq_ignore_ascii_case("text/html")
    })
}

fn request_target(uri: &Uri) -> String {
    match uri.query() {
        Some(query) => format!("{}?{query}", uri.path()),
        None => uri.path().to_string(),
    }
}

fn maintenance_location(path: &str) -> String {
    let query: String = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("continue", path)
        .finish();
    format!("/maintenance?{query}")
}

fn not_acceptable(path: &str, accept: &str) -> Response {
    (
        StatusCode::NOT_ACCEPTABLE,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        serde_json::json!({
            "message": format!("The route {path} was requested as {accept}, but only returns text/html"),
            "error": "Not Acceptable",
            "statusCode": 406,
        })
        .to_string(),
    )
        .into_response()
}

fn redirect_to(location: &str) -> Response {
    let location = HeaderValue::from_str(location)
        .unwrap_or_else(|_| HeaderValue::from_static("/maintenance"));
    (StatusCode::FOUND, [(header::LOCATION, location)]).into_response()
}

async fn ssr_share_key(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
    uri: Uri,
    headers: HeaderMap,
    Extension(index_html): Extension<Arc<String>>,
) -> Response {
    if let Some(response) = reject_unaccepted_html(uri.path(), &headers) {
        return response;
    }
    share_html(&state, index_html.as_ref(), &path, &headers, true).await
}

async fn ssr_share_slug(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
    uri: Uri,
    headers: HeaderMap,
    Extension(index_html): Extension<Arc<String>>,
) -> Response {
    if let Some(response) = reject_unaccepted_html(uri.path(), &headers) {
        return response;
    }
    share_html(&state, index_html.as_ref(), &path, &headers, false).await
}

fn reject_unaccepted_html(path: &str, headers: &HeaderMap) -> Option<Response> {
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok());
    if accepts_html(accept) {
        None
    } else {
        Some(not_acceptable(path, accept.unwrap_or("")))
    }
}

async fn share_html(
    state: &AppState,
    index_html: &str,
    id: &str,
    headers: &HeaderMap,
    by_key: bool,
) -> Response {
    if id.is_empty() {
        return html_response(StatusCode::OK, index_html.to_string());
    }

    let mut status = StatusCode::OK;
    let mut html = index_html.to_string();
    let default_domain = request_origin(headers);
    let auth = if by_key {
        state
            .services
            .auth
            .validate_shared_link_key(id, "/", &[])
            .await
    } else {
        state
            .services
            .auth
            .validate_shared_link_slug(id, "/", &[])
            .await
    };

    match auth {
        Ok(auth) => {
            match state
                .services
                .shared_link
                .get_metadata_tags(&auth, default_domain.as_deref())
                .await
            {
                Ok(Some(meta)) => html = render_og_tags(&html, &meta),
                Ok(None) => {}
                Err(_) => status = StatusCode::NOT_FOUND,
            }
        }
        Err(_) => status = StatusCode::NOT_FOUND,
    }

    html_response(status, html)
}

fn request_origin(headers: &HeaderMap) -> Option<String> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())?;
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("http");
    Some(format!("{proto}://{host}"))
}

pub fn render_og_tags(index: &str, meta: &OpenGraphTags) -> String {
    let title = escape_html(&meta.title);
    let description = escape_html(&meta.description);
    let image_url = meta
        .image_url
        .as_deref()
        .map(escape_html)
        .unwrap_or_default();
    let image = if image_url.is_empty() {
        String::new()
    } else {
        format!(r#"<meta property="og:image" content="{image_url}" />"#)
    };
    let tags = format!(
        r#"
    <meta name="description" content="{description}" />

    <!-- Facebook Meta Tags -->
    <meta property="og:type" content="website" />
    <meta property="og:title" content="{title}" />
    <meta property="og:description" content="{description}" />
    {image}"#
    );
    index.replace("<!-- metadata:tags -->", &tags)
}

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn html_response(status: StatusCode, body: String) -> Response {
    (
        status,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{SpaAction, WebMode, spa_action};
    use axum::http::Method;

    #[test]
    fn photos_page_is_html() {
        let action = spa_action(
            WebMode::App,
            &Method::GET,
            "/photos",
            "/photos",
            Some("text/html"),
        );
        assert_eq!(action, SpaAction::Html);
    }

    #[test]
    fn missing_accept_still_returns_html() {
        let action = spa_action(WebMode::App, &Method::GET, "/photos", "/photos", None);
        assert_eq!(action, SpaAction::Html);
    }

    #[test]
    fn json_accept_is_not_acceptable() {
        let action = spa_action(
            WebMode::App,
            &Method::GET,
            "/photos",
            "/photos",
            Some("application/json"),
        );
        assert!(matches!(action, SpaAction::NotAcceptable { .. }));
    }

    #[test]
    fn api_paths_are_not_the_spa() {
        let action = spa_action(
            WebMode::App,
            &Method::GET,
            "/api/timeline/buckets",
            "/api/timeline/buckets",
            Some("*/*"),
        );
        assert_eq!(action, SpaAction::NotFound);
    }

    #[test]
    fn excluded_paths_are_not_the_spa() {
        for path in ["/.well-known/immich", "/custom.css", "/favicon.ico"] {
            let action = spa_action(WebMode::App, &Method::GET, path, path, Some("text/html"));
            assert_eq!(action, SpaAction::NotFound, "{path}");
        }
    }

    #[test]
    fn maintenance_redirects_other_pages() {
        let action = spa_action(
            WebMode::Maintenance,
            &Method::GET,
            "/photos",
            "/photos",
            Some("text/html"),
        );
        assert_eq!(
            action,
            SpaAction::Redirect("/maintenance?continue=%2Fphotos".to_string())
        );
    }

    #[test]
    fn maintenance_page_is_html() {
        let action = spa_action(
            WebMode::Maintenance,
            &Method::GET,
            "/maintenance",
            "/maintenance",
            None,
        );
        assert_eq!(action, SpaAction::Html);
    }
}
