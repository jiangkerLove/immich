use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as AxumPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Router};
use tower_http::services::{ServeDir, ServeFile};

use crate::app_state::AppState;
use crate::service::shared_link::OpenGraphTags;

pub fn resolve_web_root(env: &crate::models::dto::env::EnvDto) -> Option<PathBuf> {
    if let Some(root) = env.immich_web_root.as_ref() {
        let path = PathBuf::from(root);
        if path.join("index.html").is_file() {
            return Some(path);
        }
        eprintln!(
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
    let index = web_root.join("index.html");
    let index_html = Arc::new(std::fs::read_to_string(&index).unwrap_or_default());

    let serve_dir = ServeDir::new(web_root)
        .append_index_html_on_directories(true)
        .not_found_service(ServeFile::new(index.clone()));

    Router::new()
        .route("/", get(serve_index))
        .route("/share/{*path}", get(ssr_share_key))
        .route("/s/{*path}", get(ssr_share_slug))
        .layer(Extension(index_html))
        .fallback_service(serve_dir)
}

async fn serve_index() -> Response {
    for candidate in default_web_roots() {
        let index = candidate.join("index.html");
        if let Ok(contents) = tokio::fs::read_to_string(&index).await {
            return html_response(StatusCode::OK, contents);
        }
    }

    (StatusCode::NOT_FOUND, "Web UI not found").into_response()
}

async fn ssr_share_key(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
    headers: HeaderMap,
    Extension(index_html): Extension<Arc<String>>,
) -> Response {
    let key = first_segment(&path);
    if key.is_empty() {
        return html_response(StatusCode::NOT_FOUND, index_html.as_ref().clone());
    }

    let mut status = StatusCode::OK;
    let mut html = index_html.as_ref().clone();
    let default_domain = request_origin(&headers);

    match state
        .services
        .auth
        .validate_shared_link_key(key, "/", &[])
        .await
    {
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

async fn ssr_share_slug(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
    headers: HeaderMap,
    Extension(index_html): Extension<Arc<String>>,
) -> Response {
    let slug = first_segment(&path);
    if slug.is_empty() {
        return html_response(StatusCode::NOT_FOUND, index_html.as_ref().clone());
    }

    let mut status = StatusCode::OK;
    let mut html = index_html.as_ref().clone();
    let default_domain = request_origin(&headers);

    match state
        .services
        .auth
        .validate_shared_link_slug(slug, "/", &[])
        .await
    {
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

fn first_segment(path: &str) -> &str {
    path.trim_matches('/').split('/').next().unwrap_or("")
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

    let image_facebook = if image_url.is_empty() {
        String::new()
    } else {
        format!(r#"<meta property="og:image" content="{image_url}" />"#)
    };
    let image_twitter = if image_url.is_empty() {
        String::new()
    } else {
        format!(r#"<meta name="twitter:image" content="{image_url}" />"#)
    };

    let tags = format!(
        r#"
    <meta name="description" content="{description}" />

    <!-- Facebook Meta Tags -->
    <meta property="og:type" content="website" />
    <meta property="og:title" content="{title}" />
    <meta property="og:description" content="{description}" />
    {image_facebook}

    <!-- Twitter Meta Tags -->
    <meta name="twitter:card" content="summary_large_image" />
    <meta name="twitter:title" content="{title}" />
    <meta name="twitter:description" content="{description}" />

    {image_twitter}"#
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
