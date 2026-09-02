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
        eprintln!("exiftool warning for {path}: {stderr}");
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
