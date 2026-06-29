use std::path::{Path, PathBuf};

use axum::Router;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tower_http::services::{ServeDir, ServeFile};

use crate::app_state::AppState;

pub fn resolve_web_root(env: &crate::models::dto::env::EnvDto) -> Option<PathBuf> {
    if let Some(root) = env.immich_web_root.as_ref() {
        let path = PathBuf::from(root);
        if path.join("index.html").is_file() {
            return Some(path);
        }
        eprintln!("IMMICH_WEB_ROOT={} has no index.html; static web UI disabled", root);
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
    roots.push(PathBuf::from("../server/build/www"));
    roots
}

pub fn fallback_router(web_root: &Path) -> Router<AppState> {
    let index = web_root.join("index.html");
    let serve_dir = ServeDir::new(web_root)
        .append_index_html_on_directories(true)
        .not_found_service(ServeFile::new(index.clone()));

    Router::new()
        .route("/", get(serve_index))
        .fallback_service(serve_dir)
}

async fn serve_index() -> Response {
    for candidate in default_web_roots() {
        let index = candidate.join("index.html");
        if let Ok(contents) = tokio::fs::read_to_string(&index).await {
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"))],
                contents,
            )
                .into_response();
        }
    }

    (StatusCode::NOT_FOUND, "Web UI not found").into_response()
}
