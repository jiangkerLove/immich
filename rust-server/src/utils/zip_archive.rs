use std::collections::HashMap;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Response, header};
use tempfile::NamedTempFile;
use tokio_util::io::ReaderStream;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::models::response::response::ErrorResp;
use crate::utils::file_response::{file_extension, file_stem};

pub struct ZipEntry {
    pub path: String,
    pub name: String,
}

pub async fn zip_response(entries: Vec<ZipEntry>) -> Result<Response<Body>, ErrorResp> {
    let temp = tokio::task::spawn_blocking(move || build_zip_file(entries))
        .await
        .map_err(|err| ErrorResp::ServerError(err.to_string()))??;

    let file = tokio::fs::File::open(temp.path())
        .await
        .map_err(|err| ErrorResp::ServerError(err.to_string()))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(ZipStreamBody {
        stream,
        _temp: temp,
    });

    Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .body(body)
        .map_err(|err| ErrorResp::ServerError(err.to_string()))
}

struct ZipStreamBody {
    stream: ReaderStream<tokio::fs::File>,
    _temp: NamedTempFile,
}

impl futures_util::Stream for ZipStreamBody {
    type Item = Result<axum::body::Bytes, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.stream).poll_next(cx)
    }
}

fn build_zip_file(entries: Vec<ZipEntry>) -> Result<NamedTempFile, ErrorResp> {
    let temp = NamedTempFile::new().map_err(|err| ErrorResp::ServerError(err.to_string()))?;
    let file = std::fs::File::create(temp.path())
        .map_err(|err| ErrorResp::ServerError(err.to_string()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut name_counts: HashMap<String, u32> = HashMap::new();

    for entry in entries {
        let fs_path = resolve_path(&entry.path);
        if !Path::new(&fs_path).exists() {
            continue;
        }

        let archive_name = unique_archive_name(&entry.name, &mut name_counts);
        zip.start_file(archive_name, options)
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?;
        let mut source =
            std::fs::File::open(&fs_path).map_err(|err| ErrorResp::ServerError(err.to_string()))?;
        std::io::copy(&mut source, &mut zip)
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?;
    }

    zip.finish()
        .map_err(|err| ErrorResp::ServerError(err.to_string()))?;
    Ok(temp)
}

fn resolve_path(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string())
}

pub fn archive_entry_name(original_file_name: &str, path: &str) -> String {
    format!("{}{}", file_stem(original_file_name), file_extension(path))
}

fn unique_archive_name(original: &str, counts: &mut HashMap<String, u32>) -> String {
    let mut filename = sanitize_filename(original);
    if filename.is_empty() {
        filename = "unnamed".to_string();
    }

    let count = counts.entry(filename.clone()).or_insert(0);
    let current = *count;
    *count += 1;

    if current == 0 {
        return filename;
    }

    let path = PathBuf::from(&filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("unnamed");
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    format!("{stem}+{current}{ext}")
}

fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if matches!(
                ch,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'
            ) {
                '_'
            } else {
                ch
            }
        })
        .collect();
    sanitized.trim().to_string()
}
