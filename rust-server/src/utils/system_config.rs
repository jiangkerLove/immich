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
    let mut merged = defaults();
    if let Some(stored) = get_json(pool, CONFIG_KEY).await? {
        merge_preferences(&mut merged, stored);
    }
    Ok(merged)
}

pub async fn set_config_field(pool: &PgPool, path: &[&str], value: Value) -> Result<(), sqlx::Error> {
    let mut config = get_merged(pool).await?;
    set_at(&mut config, path, value);
    set_json(pool, CONFIG_KEY, &config).await
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
    path.iter().try_fold(value, |current, key| current.get(*key))
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
