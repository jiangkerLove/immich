use std::io::SeekFrom;
use std::path::Path;

use axum::body::Body;
use axum::http::{Response, StatusCode, header};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::models::response::response::ErrorResp;
use crate::utils::mime_types;

/// Matches `CacheControl` in `server/src/enum.ts` + `utils/file.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheControl {
    PrivateWithCache,
    PrivateWithoutCache,
    /// Omit the Cache-Control header entirely (profile images).
    None,
}

impl CacheControl {
    pub fn header_value(self) -> Option<&'static str> {
        match self {
            Self::PrivateWithCache => Some(
                "private, max-age=86400, no-transform, stale-while-revalidate=2592000, stale-if-error=2592000",
            ),
            Self::PrivateWithoutCache => Some("private, no-cache, no-transform"),
            Self::None => None,
        }
    }
}

pub struct FileResponse {
    pub path: String,
    pub content_type: String,
    pub file_name: Option<String>,
    pub cache_control: CacheControl,
    /// Raw `Range` request header value, e.g. `bytes=0-1023`.
    pub range: Option<String>,
}

pub async fn file_response(file: FileResponse) -> Result<Response<Body>, ErrorResp> {
    if !Path::new(&file.path).exists() {
        return Err(ErrorResp::NotFound("Not Found".to_string()));
    }

    let content_type = file.content_type;
    let metadata = tokio::fs::metadata(&file.path)
        .await
        .map_err(|_| ErrorResp::NotFound("Not Found".to_string()))?;
    let file_len = metadata.len();

    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT_RANGES, "bytes");

    if let Some(cache_control) = file.cache_control.header_value() {
        builder = builder.header(header::CACHE_CONTROL, cache_control);
    }

    if let Some(name) = file.file_name {
        builder = builder.header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename*=UTF-8''{}", urlencoding::encode(&name)),
        );
    }

    let range = file.range.as_deref().and_then(parse_bytes_range);
    if let Some((start, end_inclusive)) = range {
        if start >= file_len {
            return builder
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{file_len}"))
                .body(Body::empty())
                .map_err(|e| ErrorResp::ServerError(e.to_string()));
        }
        let end = end_inclusive
            .unwrap_or(file_len.saturating_sub(1))
            .min(file_len.saturating_sub(1));
        if end < start {
            return builder
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{file_len}"))
                .body(Body::empty())
                .map_err(|e| ErrorResp::ServerError(e.to_string()));
        }

        let length = end - start + 1;
        // Headers not yet sent: open/read failures become 404 like TS `sendFile`.
        let mut handle = tokio::fs::File::open(&file.path)
            .await
            .map_err(|_| ErrorResp::NotFound("Not Found".to_string()))?;
        handle
            .seek(SeekFrom::Start(start))
            .await
            .map_err(|_| ErrorResp::NotFound("Not Found".to_string()))?;
        let limited = handle.take(length);
        let stream = ReaderStream::new(limited);
        let body = Body::from_stream(stream);

        return builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_LENGTH, length.to_string())
            .header(
                header::CONTENT_RANGE,
                format!("bytes {start}-{end}/{file_len}"),
            )
            .body(body)
            .map_err(|e| ErrorResp::ServerError(e.to_string()));
    }

    let handle = tokio::fs::File::open(&file.path)
        .await
        .map_err(|_| ErrorResp::NotFound("Not Found".to_string()))?;
    let stream = ReaderStream::new(handle);
    let body = Body::from_stream(stream);

    builder
        .header(header::CONTENT_LENGTH, file_len.to_string())
        .body(body)
        .map_err(|e| ErrorResp::ServerError(e.to_string()))
}

fn parse_bytes_range(header: &str) -> Option<(u64, Option<u64>)> {
    let value = header.trim();
    let rest = value.strip_prefix("bytes=")?;
    // Only the first range is supported (matches common Express sendFile behavior).
    let first = rest.split(',').next()?.trim();
    let (start_raw, end_raw) = first.split_once('-')?;
    if start_raw.is_empty() {
        // suffix ranges like bytes=-500 are uncommon for Immich playback; ignore.
        return None;
    }
    let start = start_raw.parse::<u64>().ok()?;
    let end = if end_raw.is_empty() {
        None
    } else {
        Some(end_raw.parse::<u64>().ok()?)
    };
    Some((start, end))
}

/// Content-Type from Immich mime map (first type), else `mime_guess`, else octet-stream.
pub fn guess_mime(path: &str) -> String {
    mime_types::lookup(path)
}

pub fn file_stem(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("asset");
    // Node `basename(path, extname(path))`: leading-dot names with no second
    // dot have empty stem (`.jpg` → `""`).
    if name.starts_with('.') && !name[1..].contains('.') {
        return String::new();
    }
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("asset")
        .to_string()
}

pub fn file_extension(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    // Node `extname('.gitignore')` is `""`, then Immich returns the whole name.
    if name.starts_with('.') && !name[1..].contains('.') {
        return name.to_string();
    }
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{CacheControl, file_extension, file_stem, parse_bytes_range};

    #[test]
    fn parses_open_and_closed_byte_ranges() {
        assert_eq!(parse_bytes_range("bytes=0-1023"), Some((0, Some(1023))));
        assert_eq!(parse_bytes_range("bytes=100-"), Some((100, None)));
        assert_eq!(parse_bytes_range("bytes=0-0,1-2"), Some((0, Some(0))));
        assert_eq!(parse_bytes_range("bytes=-500"), None);
    }

    #[test]
    fn cache_control_none_omits_header() {
        assert!(CacheControl::None.header_value().is_none());
        assert!(
            CacheControl::PrivateWithCache
                .header_value()
                .unwrap()
                .contains("max-age=86400")
        );
    }

    #[test]
    fn dotfile_extension_matches_node_extname_helper() {
        assert_eq!(file_extension(".gitignore"), ".gitignore");
        assert_eq!(file_stem(".gitignore"), "");
        assert_eq!(file_extension(".jpg"), ".jpg");
        assert_eq!(file_stem(".jpg"), "");
        assert_eq!(file_extension("photo.jpg"), ".jpg");
        assert_eq!(file_stem("photo.jpg"), "photo");
    }
}
