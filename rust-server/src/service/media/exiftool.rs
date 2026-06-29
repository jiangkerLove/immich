use std::process::Stdio;

use serde_json::Value;
use tokio::process::Command;

pub async fn read_tags(path: &str, extended_video: bool) -> Result<Value, String> {
    let mut command = Command::new("exiftool");
    command
        .arg("-api")
        .arg("largefilesupport=1")
        .arg("-json")
        .arg("-struct")
        .arg("-n")
        .arg("-charset")
        .arg("filename=utf8");
    if extended_video {
        command.arg("-ee");
    }
    command.arg(path);

    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|err| format!("failed to run exiftool: {err}"))?;

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
    let output = Command::new("exiftool")
        .arg("-b")
        .arg(format!("-{tag_name}"))
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|err| format!("failed to run exiftool: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("exiftool binary extract failed for {tag_name}: {stderr}"));
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

    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|err| format!("failed to run exiftool: {err}"))?;

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
