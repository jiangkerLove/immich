use std::process::{Output, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;
use tokio::sync::{RwLock, Semaphore};

const DEFAULT_CONCURRENCY: usize = 5;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

static EXECUTION_LIMIT: OnceLock<RwLock<Arc<Semaphore>>> = OnceLock::new();

/// Limits concurrent ExifTool invocations across metadata and sidecar workers.
///
/// This is deliberately configured from `job.metadataExtraction.concurrency` so
/// worker restarts cannot create an unbounded number of external processes.
pub async fn configure_concurrency(max_processes: usize) {
    let limit =
        EXECUTION_LIMIT.get_or_init(|| RwLock::new(Arc::new(Semaphore::new(DEFAULT_CONCURRENCY))));
    *limit.write().await = Arc::new(Semaphore::new(max_processes.max(1)));
}

async fn run(command: &mut Command) -> Result<Output, String> {
    let limit = EXECUTION_LIMIT
        .get_or_init(|| RwLock::new(Arc::new(Semaphore::new(DEFAULT_CONCURRENCY))))
        .read()
        .await
        .clone();
    let _permit = limit
        .acquire_owned()
        .await
        .map_err(|_| "ExifTool execution limiter is closed".to_string())?;

    tokio::time::timeout(REQUEST_TIMEOUT, command.output())
        .await
        .map_err(|_| "ExifTool request timed out after 120 seconds".to_string())?
        .map_err(|err| format!("failed to run exiftool: {err}"))
}

pub async fn read_tags(path: &str, extended_video: bool) -> Result<Value, String> {
    let mut command = Command::new("exiftool");
    command
        .arg("-api")
        .arg("largefilesupport=1")
        .arg("-json")
        .arg("-struct")
        .arg("-n")
        .arg("-charset")
        .arg("filename=utf8")
        .arg("--ICC_Profile:DeviceManufacturer")
        .arg("--ICC_Profile:DeviceModelName");
    if extended_video {
        command.arg("-ee");
    }
    command.arg(path);

    let output = run(command.stdout(Stdio::piped()).stderr(Stdio::piped())).await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!("exiftool warning for {path}: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Value = serde_json::from_str(&stdout).unwrap_or(Value::Null);
    Ok(parsed
        .as_array()
        .and_then(|items| items.first().cloned())
        .unwrap_or(Value::Object(Default::default())))
}

pub fn tag_value(tags: &Value, name: &str) -> Option<Value> {
    if let Some(obj) = tags.as_object() {
        if let Some(value) = obj.get(name) {
            if !value.is_null() {
                return Some(value.clone());
            }
        }
        for (key, value) in obj {
            if key.ends_with(&format!(":{name}")) && !value.is_null() {
                return Some(value.clone());
            }
        }
    }
    None
}

pub fn tag_string(tags: &Value, name: &str) -> Option<String> {
    tag_value(tags, name).and_then(|value| match value {
        Value::String(s) if !s.is_empty() => Some(s),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

pub fn tag_f64(tags: &Value, name: &str) -> Option<f64> {
    tag_value(tags, name).and_then(|value| match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    })
}

pub fn tag_i32(tags: &Value, name: &str) -> Option<i32> {
    tag_f64(tags, name).map(|v| v.round() as i32)
}

/// TypeScript `validate`: use the first element of a list, then keep a finite
/// integer-range number. Numeric strings are still parsed because exiftool JSON
/// emits them that way.
pub fn tag_validated_f64(tags: &Value, name: &str) -> Option<f64> {
    tag_value(tags, name).and_then(|value| validated_f64(&value))
}

pub fn tag_validated_i32(tags: &Value, name: &str) -> Option<i32> {
    tag_validated_f64(tags, name).map(|value| value.round() as i32)
}

fn validated_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Array(items) => items.first().and_then(validated_f64),
        Value::Number(number) => number.as_f64().and_then(postgres_int_f64),
        Value::String(text) => text
            .split_whitespace()
            .next()
            .and_then(|token| token.parse::<f64>().ok())
            .and_then(postgres_int_f64),
        _ => None,
    }
}

fn postgres_int_f64(value: f64) -> Option<f64> {
    if value.is_finite() && (-2_147_483_648.0..=2_147_483_647.0).contains(&value) {
        Some(value)
    } else {
        None
    }
}

pub fn tag_string_list(tags: &Value, name: &str) -> Vec<String> {
    let Some(value) = tag_value(tags, name) else {
        return Vec::new();
    };
    match value {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        Value::String(s) if !s.is_empty() => vec![s],
        Value::Number(n) => vec![n.to_string()],
        _ => Vec::new(),
    }
}

pub async fn extract_binary_tag(path: &str, tag_name: &str) -> Result<Vec<u8>, String> {
    let mut command = Command::new("exiftool");
    command
        .arg("-b")
        .arg(format!("-{tag_name}"))
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = run(&mut command).await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "exiftool binary extract failed for {tag_name}: {stderr}"
        ));
    }

    Ok(output.stdout)
}

/// Copy one ExifTool group from `source` onto `target`.
///
/// Matches TypeScript `MediaRepository.copyTagGroup` (`-m`, `-TagsFromFile`,
/// `-{group}:all>{group}:all`, `-overwrite_original`).
pub async fn copy_tag_group(tag_group: &str, source: &str, target: &str) -> Result<(), String> {
    let mut command = Command::new("exiftool");
    for arg in copy_tag_group_args(tag_group, source) {
        command.arg(arg);
    }
    command
        .arg(target)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = run(&mut command).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "exiftool copy tag group {tag_group} failed for {target}: {stderr}"
        ));
    }

    Ok(())
}

/// Copy the source ICC profile onto a derivative.
///
/// ExifTool exits successfully when the source has no profile. Callers treat a
/// hard failure as a warning so thumbnail generation still completes.
pub async fn copy_icc_profile(source: &str, target: &str) -> Result<(), String> {
    let mut command = Command::new("exiftool");
    command
        .arg("-m")
        .arg("-overwrite_original")
        .arg("-TagsFromFile")
        .arg(source)
        .arg("-icc_profile")
        .arg(target)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = run(&mut command).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "exiftool copy icc profile failed for {target}: {stderr}"
        ));
    }
    Ok(())
}

pub(crate) fn orientation_colorspace_args(
    orientation: Option<&str>,
    colorspace: Option<&str>,
) -> Vec<String> {
    let mut args = vec!["-m".to_string(), "-overwrite_original".to_string()];
    if let Some(orientation) = orientation.and_then(|value| value.parse::<i32>().ok()) {
        if orientation > 0 {
            args.push(format!("-Orientation#={orientation}"));
        }
    }
    if let Some(colorspace) = colorspace.filter(|value| !value.is_empty()) {
        args.push(format!("-ColorSpace={colorspace}"));
    }
    args
}

pub async fn write_orientation_and_colorspace(
    path: &str,
    orientation: Option<&str>,
    colorspace: Option<&str>,
) -> Result<(), String> {
    let args = orientation_colorspace_args(orientation, colorspace);
    if args.len() == 2 {
        return Ok(());
    }

    let mut command = Command::new("exiftool");
    command
        .arg("-api")
        .arg("largefilesupport=1")
        .args(&args)
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = run(&mut command).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("exiftool write exif failed for {path}: {stderr}"));
    }
    Ok(())
}

pub(crate) fn copy_tag_group_args(tag_group: &str, source: &str) -> Vec<String> {
    vec![
        "-m".to_string(),
        "-overwrite_original".to_string(),
        "-TagsFromFile".to_string(),
        source.to_string(),
        format!("-{tag_group}:all>{tag_group}:all"),
    ]
}

pub async fn write_tags(path: &str, tags: &[(&str, TagWriteValue)]) -> Result<(), String> {
    if tags.is_empty() {
        return Ok(());
    }

    if let Some(parent) = std::path::Path::new(path).parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| err.to_string())?;
    }

    let mut command = Command::new("exiftool");
    command
        .arg("-api")
        .arg("largefilesupport=1")
        .arg("-overwrite_original");

    for (name, value) in tags {
        match value {
            TagWriteValue::Text(text) => {
                command.arg(format!("-{name}^={text}"));
            }
            TagWriteValue::Number(number) => {
                command.arg(format!("-{name}^={number}"));
            }
            TagWriteValue::StringList(items) => {
                for item in items {
                    command.arg(format!("-{name}^={item}"));
                }
            }
        }
    }

    command.arg(path);

    let output = run(command.stdout(Stdio::piped()).stderr(Stdio::piped())).await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("exiftool write failed for {path}: {stderr}"));
    }

    Ok(())
}

pub enum TagWriteValue {
    Text(String),
    Number(f64),
    StringList(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::{copy_tag_group_args, orientation_colorspace_args};

    #[test]
    fn copy_tag_group_args_match_typescript_write_args() {
        let args = copy_tag_group_args("XMP-GPano", "/library/original.jpg");
        assert_eq!(
            args,
            vec![
                "-m".to_string(),
                "-overwrite_original".to_string(),
                "-TagsFromFile".to_string(),
                "/library/original.jpg".to_string(),
                "-XMP-GPano:all>XMP-GPano:all".to_string(),
            ]
        );
    }

    #[test]
    fn orientation_colorspace_args_match_typescript_write_exif() {
        assert_eq!(
            orientation_colorspace_args(Some("6"), Some("sRGB")),
            vec![
                "-m".to_string(),
                "-overwrite_original".to_string(),
                "-Orientation#=6".to_string(),
                "-ColorSpace=sRGB".to_string(),
            ]
        );
        assert_eq!(
            orientation_colorspace_args(Some("0"), Some("")),
            vec!["-m".to_string(), "-overwrite_original".to_string()]
        );
    }
}
