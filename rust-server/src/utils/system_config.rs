use serde_json::Value;
use sqlx::PgPool;

use crate::models::db::system_metadata::{get_json, set_json};
use crate::utils::preferences::merge_preferences;

const CONFIG_KEY: &str = "system-config";
const DEFAULTS_JSON: &str = include_str!("../../config/system_config_defaults.json");

pub fn defaults() -> Value {
    serde_json::from_str(DEFAULTS_JSON).unwrap_or_else(|_| Value::Object(Default::default()))
}

pub async fn get_merged(pool: &PgPool) -> Result<Value, sqlx::Error> {
    let base = defaults();
    let mut merged = base.clone();
    if let Some(path) = config_file_path() {
        let partial = load_config_file(&path).map_err(|err| {
            tracing::error!("Unable to load configuration file: {path}: {err}");
            sqlx::Error::Configuration(err.into())
        })?;
        warn_unknown_keys(&base, &partial);
        merge_preferences(&mut merged, partial);
        return finish_merged_config(true, merged);
    }
    if let Some(stored) = get_json(pool, CONFIG_KEY).await? {
        warn_unknown_keys(&base, &stored);
        merge_preferences(&mut merged, stored);
    }
    finish_merged_config(false, merged)
}

fn finish_merged_config(from_file: bool, mut merged: Value) -> Result<Value, sqlx::Error> {
    let base = defaults();
    match crate::service::config_validate::normalize_admin_config(&base, &merged) {
        Ok(mut normalized) => {
            normalize_runtime_config(&mut normalized)?;
            Ok(normalized)
        }
        Err(issues) => {
            let text = crate::models::response::response::format_system_config_file_error(&issues);
            if from_file {
                tracing::error!("{text}");
                return Err(sqlx::Error::Configuration(text.into()));
            }
            tracing::error!("Validation error\n{text}");
            normalize_runtime_config(&mut merged)?;
            Ok(merged)
        }
    }
}

/// Store only values that differ from defaults, matching TS `updateConfig`.
pub fn diff_from_defaults(defaults: &Value, incoming: &Value) -> Value {
    match (defaults, incoming) {
        (Value::Object(default_map), Value::Object(incoming_map)) => {
            let mut out = serde_json::Map::new();
            for (key, default_value) in default_map {
                let Some(incoming_value) = incoming_map.get(key) else {
                    continue;
                };
                if is_empty_config_value(incoming_value) || incoming_value == default_value {
                    continue;
                }
                if default_value.is_object() && incoming_value.is_object() {
                    let nested = diff_from_defaults(default_value, incoming_value);
                    if nested.as_object().is_some_and(|map| !map.is_empty()) {
                        out.insert(key.clone(), nested);
                    }
                } else {
                    out.insert(key.clone(), incoming_value.clone());
                }
            }
            Value::Object(out)
        }
        _ => incoming.clone(),
    }
}

fn config_file_path() -> Option<String> {
    std::env::var("IMMICH_CONFIG_FILE")
        .ok()
        .filter(|path| !path.is_empty())
}

fn load_config_file(path: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_yaml::from_str(&text).map_err(|err| err.to_string())
}

fn is_empty_config_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) if text.is_empty() => true,
        _ => false,
    }
}

fn warn_unknown_keys(defaults: &Value, partial: &Value) {
    let unknown = unknown_keys(defaults, partial);
    if unknown.as_object().is_some_and(|map| !map.is_empty()) {
        tracing::warn!("Unknown keys found: {unknown}");
    }
}

fn unknown_keys(defaults: &Value, partial: &Value) -> Value {
    match (defaults, partial) {
        (Value::Object(default_map), Value::Object(partial_map)) => {
            let mut out = serde_json::Map::new();
            for (key, value) in partial_map {
                match default_map.get(key) {
                    None => {
                        out.insert(key.clone(), value.clone());
                    }
                    Some(default_value) if default_value.is_object() && value.is_object() => {
                        let nested = unknown_keys(default_value, value);
                        if nested.as_object().is_some_and(|map| !map.is_empty()) {
                            out.insert(key.clone(), nested);
                        }
                    }
                    _ => {}
                }
            }
            Value::Object(out)
        }
        _ => Value::Object(serde_json::Map::new()),
    }
}

fn normalize_runtime_config(config: &mut Value) -> Result<(), sqlx::Error> {
    normalize_external_domain(config)?;
    ensure_codec_listed(config, "acceptedVideoCodecs", "targetVideoCodec");
    ensure_codec_listed(config, "acceptedAudioCodecs", "targetAudioCodec");
    Ok(())
}

fn normalize_external_domain(config: &mut Value) -> Result<(), sqlx::Error> {
    let Some(domain) = config
        .pointer("/server/externalDomain")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    else {
        return Ok(());
    };
    let url = url::Url::parse(&domain).map_err(|err| {
        sqlx::Error::Configuration(format!("Invalid external domain: {err}").into())
    })?;
    let normalized = if !url.username().is_empty() && url.password().is_some() {
        let host = match (url.host_str(), url.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_string(),
            _ => String::new(),
        };
        format!(
            "{}://{}:{}@{host}",
            url.scheme(),
            url.username(),
            url.password().unwrap_or("")
        )
    } else {
        url.origin().ascii_serialization()
    };
    if let Some(server) = config
        .get_mut("server")
        .and_then(|value| value.as_object_mut())
    {
        server.insert("externalDomain".to_string(), Value::String(normalized));
    }
    Ok(())
}

fn ensure_codec_listed(config: &mut Value, list_key: &str, target_key: &str) {
    let Some(ffmpeg) = config
        .get_mut("ffmpeg")
        .and_then(|value| value.as_object_mut())
    else {
        return;
    };
    let Some(target) = ffmpeg
        .get(target_key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
    else {
        return;
    };
    let Some(list) = ffmpeg
        .get_mut(list_key)
        .and_then(|value| value.as_array_mut())
    else {
        return;
    };
    if !list
        .iter()
        .any(|value| value.as_str() == Some(target.as_str()))
    {
        list.push(Value::String(target));
    }
}

pub async fn set_config_field(
    pool: &PgPool,
    path: &[&str],
    value: Value,
) -> Result<(), sqlx::Error> {
    let mut config = get_merged(pool).await?;
    set_at(&mut config, path, value);
    let partial = diff_from_defaults(&defaults(), &config);
    set_json(pool, CONFIG_KEY, &partial).await
}

fn set_at(value: &mut Value, path: &[&str], new_value: Value) {
    if path.is_empty() {
        return;
    }
    if path.len() == 1 {
        if let Value::Object(map) = value {
            map.insert(path[0].to_string(), new_value);
        }
        return;
    }
    if let Value::Object(map) = value {
        let entry = map
            .entry(path[0].to_string())
            .or_insert_with(|| Value::Object(Default::default()));
        set_at(entry, &path[1..], new_value);
    }
}

pub fn json_bool(value: &Value, path: &[&str], default: bool) -> bool {
    get_at(value, path)
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

pub fn json_i32(value: &Value, path: &[&str], default: i32) -> i32 {
    get_at(value, path)
        .and_then(|v| v.as_i64())
        .and_then(|v| i32::try_from(v).ok())
        .unwrap_or(default)
}

pub fn json_str(value: &Value, path: &[&str], default: &str) -> String {
    get_at(value, path)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

fn get_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |current, key| current.get(*key))
}

pub fn is_machine_learning_enabled(ml: &Value) -> bool {
    ml.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false)
}

pub fn is_smart_search_enabled(ml: &Value) -> bool {
    is_machine_learning_enabled(ml)
        && ml
            .get("clip")
            .and_then(|clip| clip.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
}

pub fn is_ocr_enabled(ml: &Value) -> bool {
    is_machine_learning_enabled(ml)
        && ml
            .get("ocr")
            .and_then(|ocr| ocr.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
}

pub fn is_facial_recognition_enabled(ml: &Value) -> bool {
    is_machine_learning_enabled(ml)
        && ml
            .get("facialRecognition")
            .and_then(|fr| fr.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
}

pub fn is_duplicate_detection_enabled(ml: &Value) -> bool {
    is_smart_search_enabled(ml)
        && ml
            .get("duplicateDetection")
            .and_then(|dd| dd.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_drops_defaults_and_empty_strings() {
        let defaults = serde_json::json!({
            "server": { "externalDomain": "", "publicUsers": true },
            "ffmpeg": { "crf": 23 }
        });
        let incoming = serde_json::json!({
            "server": { "externalDomain": "", "publicUsers": false },
            "ffmpeg": { "crf": 23 }
        });
        let partial = diff_from_defaults(&defaults, &incoming);
        assert_eq!(
            partial,
            serde_json::json!({ "server": { "publicUsers": false } })
        );
    }

    #[test]
    fn external_domain_strips_path_and_keeps_credentials() {
        let mut config = serde_json::json!({
            "server": { "externalDomain": "https://user:secret@immich.example:2283/photos" }
        });
        normalize_external_domain(&mut config).unwrap();
        assert_eq!(
            config["server"]["externalDomain"],
            "https://user:secret@immich.example:2283"
        );

        let mut plain = serde_json::json!({
            "server": { "externalDomain": "https://immich.example/photos" }
        });
        normalize_external_domain(&mut plain).unwrap();
        assert_eq!(plain["server"]["externalDomain"], "https://immich.example");
    }

    #[test]
    fn target_codec_is_added_to_accepted_list() {
        let mut config = serde_json::json!({
            "ffmpeg": {
                "targetVideoCodec": "hevc",
                "acceptedVideoCodecs": ["h264"]
            }
        });
        ensure_codec_listed(&mut config, "acceptedVideoCodecs", "targetVideoCodec");
        assert_eq!(
            config["ffmpeg"]["acceptedVideoCodecs"],
            serde_json::json!(["h264", "hevc"])
        );
    }
}
