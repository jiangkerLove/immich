use std::path::Path;

use axum::body::Body;
use axum::http::{header, Response};
use tokio_util::io::ReaderStream;

use crate::models::response::response::ErrorResp;

pub struct FileResponse {
    pub path: String,
    pub content_type: String,
    pub file_name: Option<String>,
}

pub async fn file_response(file: FileResponse) -> Result<Response<Body>, ErrorResp> {
    if !Path::new(&file.path).exists() {
        return Err(ErrorResp::BadRequest("Asset media not found".to_string()));
    }

    let content_type = file.content_type;
    let handle = tokio::fs::File::open(&file.path)
        .await
        .map_err(|_| ErrorResp::ServerError("Unable to read file".to_string()))?;
    let stream = ReaderStream::new(handle);
    let body = Body::from_stream(stream);

    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CACHE_CONTROL,
            "private, max-age=86400, no-transform, stale-while-revalidate=2592000",
        );

    if let Some(name) = file.file_name {
        builder = builder.header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename*=UTF-8''{}", urlencoding::encode(&name)),
        );
    }

    builder
        .body(body)
        .map_err(|e| ErrorResp::ServerError(e.to_string()))
}

pub fn guess_mime(path: &str) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string()
}

pub fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("asset")
        .to_string()
}

pub fn file_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default()
}
