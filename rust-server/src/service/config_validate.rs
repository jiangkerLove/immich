use serde_json::Value;

use crate::models::response::response::{ConfigIssue, ErrorResp};
use crate::service::bootstrap;
use crate::service::email::{EmailService, SmtpTransportConfig};
use crate::service::media::storage_template::StorageTemplateService;
use crate::utils::clip::get_clip_dim_size;
pub async fn validate_system_config(
    old_config: &Value,
    new_config: &Value,
) -> Result<Value, ErrorResp> {
    let env = bootstrap::load_env();

    if env
        .immich_config_file
        .as_ref()
        .is_some_and(|path| !path.is_empty())
    {
        return Err(ErrorResp::BadRequest(
            "Cannot update configuration while IMMICH_CONFIG_FILE is in use".to_string(),
        ));
    }

    let normalized = normalize_admin_config(&crate::utils::system_config::defaults(), new_config)
        .map_err(ErrorResp::Validation)?;

    if env.immich_log_level.is_some()
        && !json_equal(old_config.get("logging"), normalized.get("logging"))
    {
        return Err(ErrorResp::BadRequest(
            "Logging cannot be changed while the environment variable IMMICH_LOG_LEVEL is set."
                .to_string(),
        ));
    }

    if let Some(model_name) = normalized
        .get("machineLearning")
        .and_then(|ml| ml.get("clip"))
        .and_then(|clip| clip.get("modelName"))
        .and_then(|value| value.as_str())
    {
        get_clip_dim_size(model_name).map_err(|_| {
            ErrorResp::BadRequest(format!(
                "Unknown CLIP model: {model_name}. Please check the model name for typos and confirm this is a supported model."
            ))
        })?;
    }

    if let Some(template) = normalized
        .get("storageTemplate")
        .and_then(|value| value.get("template"))
        .and_then(|value| value.as_str())
    {
        StorageTemplateService::validate_storage_template(template)
            .map_err(|_| ErrorResp::BadRequest("Invalid storage template".to_string()))?;
    }

    let mobile_override = normalized
        .pointer("/oauth/mobileOverrideEnabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let mobile_redirect = normalized
        .pointer("/oauth/mobileRedirectUri")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if mobile_override && !mobile_redirect.is_empty() && url::Url::parse(mobile_redirect).is_err() {
        return Err(ErrorResp::BadRequest(
            "Mobile redirect URI must be an empty string or a valid URL".to_string(),
        ));
    }

    let smtp_enabled = normalized
        .pointer("/notifications/smtp/enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if smtp_enabled
        && !json_equal(
            old_config
                .get("notifications")
                .and_then(|value| value.get("smtp")),
            normalized
                .get("notifications")
                .and_then(|value| value.get("smtp")),
        )
    {
        let transport = normalized
            .pointer("/notifications/smtp/transport")
            .cloned()
            .ok_or_else(|| ErrorResp::BadRequest("Invalid SMTP configuration".to_string()))?;
        let transport: SmtpTransportConfig = serde_json::from_value(transport)
            .map_err(|_| ErrorResp::BadRequest("Invalid SMTP configuration".to_string()))?;
        EmailService::verify_smtp(&transport).await?;
    }

    Ok(normalized)
}

pub(crate) fn normalize_admin_config(
    defaults: &Value,
    incoming: &Value,
) -> Result<Value, Vec<ConfigIssue>> {
    walk_config(defaults, incoming, "")
}

fn walk_config(defaults: &Value, incoming: &Value, path: &str) -> Result<Value, Vec<ConfigIssue>> {
    match (defaults, incoming) {
        (Value::Object(default_map), Value::Object(incoming_map)) => {
            let mut out = serde_json::Map::new();
            let mut issues = Vec::new();
            for (key, default_value) in default_map {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if let Some(value) = incoming_map.get(key) {
                    match walk_config(default_value, value, &child) {
                        Ok(value) => {
                            out.insert(key.clone(), value);
                        }
                        Err(mut child_issues) => issues.append(&mut child_issues),
                    }
                } else if is_optional(field_path(&child)) {
                    out.insert(key.clone(), default_value.clone());
                } else {
                    issues.extend(type_issue(
                        &child,
                        expected_name(&child, default_value),
                        "undefined",
                    ));
                }
            }
            if issues.is_empty() {
                Ok(Value::Object(out))
            } else {
                Err(issues)
            }
        }
        (Value::Array(default_items), Value::Array(items)) => {
            let items = if field_path(path) == "machineLearning.urls" && items.is_empty() {
                default_items
            } else {
                items
            };
            if field_path(path) == "machineLearning.urls" && items.is_empty() {
                return Err(issue(path, "Too small: expected array to have >=1 items"));
            }
            let Some(sample) = default_items.first() else {
                return Ok(Value::Array(items.clone()));
            };
            let mut out = Vec::with_capacity(items.len());
            let mut issues = Vec::new();
            for (index, item) in items.iter().enumerate() {
                let child = format!("{path}.{index}");
                match walk_config(sample, item, &child) {
                    Ok(value) => out.push(value),
                    Err(mut child_issues) => issues.append(&mut child_issues),
                }
            }
            if issues.is_empty() {
                Ok(Value::Array(out))
            } else {
                Err(issues)
            }
        }
        (Value::Bool(_), Value::Bool(value)) => Ok(Value::Bool(*value)),
        (Value::Bool(_), Value::String(text))
            if bool_allows_string(field_path(path)) && (text == "true" || text == "false") =>
        {
            Ok(Value::Bool(text == "true"))
        }
        (Value::Number(_), Value::String(text)) if coerce_int_path(field_path(path)) => {
            if let Ok(parsed) = text.parse::<i64>() {
                let number = serde_json::Number::from(parsed);
                validate_number(path, &number)?;
                Ok(Value::Number(number))
            } else if text.parse::<f64>().is_ok() {
                Err(type_issue(path, "int", "number"))
            } else {
                Err(type_issue(path, "number", "NaN"))
            }
        }
        (Value::String(_), Value::String(text)) => {
            validate_string(path, text)?;
            Ok(Value::String(text.clone()))
        }
        (Value::Number(_), Value::Number(number)) => {
            if is_int_field(path) && number.as_i64().is_none() && number.as_u64().is_none() {
                return Err(type_issue(path, "int", "number"));
            }
            validate_number(path, number)?;
            Ok(Value::Number(number.clone()))
        }
        (Value::Null, Value::Null) => Ok(Value::Null),
        (Value::Null, Value::Number(number)) => {
            if number.as_i64().is_none() && number.as_u64().is_none() {
                return Err(type_issue(path, "int", "number"));
            }
            validate_number(path, number)?;
            Ok(Value::Number(number.clone()))
        }
        _ => Err(type_issue_input(
            path,
            expected_name(path, defaults),
            received_name(incoming),
            Some(incoming.clone()),
        )),
    }
}

fn field_path(path: &str) -> &str {
    match path.rsplit_once('.') {
        Some((parent, index))
            if !index.is_empty() && index.chars().all(|ch| ch.is_ascii_digit()) =>
        {
            parent
        }
        _ => path,
    }
}

fn path_segments(path: &str) -> Vec<Value> {
    if path.is_empty() {
        return Vec::new();
    }
    path.split('.')
        .map(|segment| {
            if !segment.is_empty() && segment.chars().all(|ch| ch.is_ascii_digit()) {
                Value::Number(segment.parse::<u64>().unwrap_or(0).into())
            } else {
                Value::String(segment.to_string())
            }
        })
        .collect()
}

fn issue(path: &str, message: impl Into<String>) -> Vec<ConfigIssue> {
    vec![ConfigIssue {
        code: "custom".to_string(),
        path: path_segments(path),
        message: message.into(),
        expected: None,
        input: None,
        values: None,
        origin: None,
        minimum: None,
        maximum: None,
        inclusive: None,
    }]
}

fn type_issue(path: &str, expected: &str, received: &str) -> Vec<ConfigIssue> {
    type_issue_input(path, expected, received, None)
}

fn type_issue_input(
    path: &str,
    expected: &str,
    received: &str,
    input: Option<Value>,
) -> Vec<ConfigIssue> {
    vec![ConfigIssue {
        code: "invalid_type".to_string(),
        path: path_segments(path),
        message: format!("Invalid input: expected {expected}, received {received}"),
        expected: Some(expected.to_string()),
        input,
        values: None,
        origin: None,
        minimum: None,
        maximum: None,
        inclusive: None,
    }]
}

fn enum_issue(path: &str, allowed: &[&str], input: &str) -> Vec<ConfigIssue> {
    let options = allowed
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join("|");
    vec![ConfigIssue {
        code: "invalid_value".to_string(),
        path: path_segments(path),
        message: format!("Invalid option: expected one of {options}"),
        expected: None,
        input: Some(Value::String(input.to_string())),
        values: Some(allowed.iter().map(|value| (*value).to_string()).collect()),
        origin: None,
        minimum: None,
        maximum: None,
        inclusive: None,
    }]
}

fn bounds_issue(
    path: &str,
    code: &str,
    message: String,
    input: f64,
    minimum: Option<f64>,
    maximum: Option<f64>,
) -> Vec<ConfigIssue> {
    vec![ConfigIssue {
        code: code.to_string(),
        path: path_segments(path),
        message,
        expected: None,
        input: Some(serde_json::json!(input)),
        values: None,
        origin: Some("number".to_string()),
        minimum,
        maximum,
        inclusive: Some(true),
    }]
}

fn expected_name(path: &str, default: &Value) -> &'static str {
    match default {
        Value::Bool(_) => "boolean",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "int",
        Value::Number(_) if is_int_field(path) => "int",
        Value::Number(_) => "number",
    }
}

fn received_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn is_int_field(path: &str) -> bool {
    float_bounds(field_path(path)).is_none()
}

fn bool_allows_string(path: &str) -> bool {
    !matches!(
        path,
        "integrityChecks.missingFiles.enabled"
            | "integrityChecks.untrackedFiles.enabled"
            | "integrityChecks.checksumFiles.enabled"
            | "machineLearning.clip.enabled"
            | "machineLearning.duplicateDetection.enabled"
            | "machineLearning.facialRecognition.enabled"
            | "machineLearning.ocr.enabled"
    )
}

fn coerce_int_path(path: &str) -> bool {
    matches!(
        path,
        "ffmpeg.crf" | "ffmpeg.threads" | "ffmpeg.bframes" | "ffmpeg.refs" | "ffmpeg.gopSize"
    )
}

fn is_optional(path: &str) -> bool {
    matches!(
        path,
        "image.thumbnail.progressive"
            | "image.preview.progressive"
            | "image.fullsize.progressive"
            | "oauth.accountManagementUrl"
    )
}

fn validate_string(path: &str, text: &str) -> Result<(), Vec<ConfigIssue>> {
    let path_key = field_path(path);
    if path_key == "nightlyTasks.startTime" && !is_hh_mm(text) {
        return Err(issue(
            path,
            "Invalid input: expected string in HH:MM format, received string",
        ));
    }
    if path_key.ends_with("cronExpression") {
        if let Err(message) = cron_error(text) {
            return Err(issue(path, message));
        }
    }
    if let Some(allowed) = string_enum(path_key) {
        if !allowed.contains(&text) {
            return Err(enum_issue(path, allowed, text));
        }
    }
    if let Some(message) = empty_or_url_message(path_key) {
        if !text.is_empty() && url::Url::parse(text).is_err() {
            return Err(issue(path, message));
        }
    }
    if strict_url_field(path_key) && url::Url::parse(text).is_err() {
        return Err(issue(path, "Invalid URL"));
    }
    Ok(())
}

fn validate_number(path: &str, number: &serde_json::Number) -> Result<(), Vec<ConfigIssue>> {
    let path_key = field_path(path);
    if let Some((min, max)) = float_bounds(path_key) {
        let Some(value) = number.as_f64() else {
            return Err(type_issue(path, "number", "number"));
        };
        return check_bounds(path, value, min, max);
    }

    if path_key == "ffmpeg.realtime.resolutions" {
        let Some(value) = number.as_i64() else {
            return Err(type_issue(path, "int", "number"));
        };
        if !matches!(value, 480 | 720 | 1080 | 1440 | 2160) {
            return Err(issue(
                path,
                "Invalid option: expected one of 480|720|1080|1440|2160",
            ));
        }
        return Ok(());
    }

    let Some(value) = number.as_i64() else {
        return Err(type_issue(path, "int", "number"));
    };
    if let Some((min, max)) = int_bounds(path_key) {
        return check_bounds(path, value as f64, min as f64, max as f64);
    }
    Ok(())
}

fn check_bounds(path: &str, value: f64, min: f64, max: f64) -> Result<(), Vec<ConfigIssue>> {
    if value < min {
        return Err(bounds_issue(
            path,
            "too_small",
            format!("Too small: expected number to be >={}", format_bound(min)),
            value,
            Some(min),
            None,
        ));
    }
    if max < 1e15 && value > max {
        return Err(bounds_issue(
            path,
            "too_big",
            format!("Too big: expected number to be <={}", format_bound(max)),
            value,
            None,
            Some(max),
        ));
    }
    Ok(())
}

fn format_bound(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

fn float_bounds(path: &str) -> Option<(f64, f64)> {
    match path {
        "machineLearning.duplicateDetection.maxDistance" => Some((0.001, 0.1)),
        "machineLearning.facialRecognition.minScore" => Some((0.1, 1.0)),
        "machineLearning.facialRecognition.maxDistance" => Some((0.1, 2.0)),
        "machineLearning.ocr.minDetectionScore" | "machineLearning.ocr.minRecognitionScore" => {
            Some((0.1, 1.0))
        }
        "integrityChecks.checksumFiles.percentageLimit" => Some((0.0, 1.0)),
        _ => None,
    }
}

fn int_bounds(path: &str) -> Option<(i64, i64)> {
    if path.ends_with(".concurrency") || path.ends_with(".quality") && path.contains("image.") {
        return Some(if path.ends_with(".quality") {
            (1, 100)
        } else {
            (1, i64::MAX)
        });
    }
    match path {
        "ffmpeg.crf" => Some((0, 51)),
        "ffmpeg.threads" | "ffmpeg.gopSize" | "trash.days" => Some((0, i64::MAX)),
        "ffmpeg.bframes" => Some((-1, 16)),
        "ffmpeg.refs" => Some((0, 6)),
        "notifications.smtp.transport.port" => Some((0, 65_535)),
        "backup.database.keepLastAmount"
        | "image.preview.size"
        | "image.thumbnail.size"
        | "image.fullsize.size"
        | "machineLearning.facialRecognition.minFaces"
        | "machineLearning.ocr.maxResolution"
        | "oauth.timeout"
        | "user.deleteDelay" => Some((1, i64::MAX)),
        "oauth.defaultStorageQuota" | "integrityChecks.checksumFiles.timeLimit" => {
            Some((0, i64::MAX))
        }
        _ => None,
    }
}

fn string_enum(path: &str) -> Option<&'static [&'static str]> {
    const IMAGE_FORMAT: &[&str] = &["jpeg", "webp"];
    const COLORSPACE: &[&str] = &["srgb", "p3"];
    const VIDEO: &[&str] = &["h264", "hevc", "vp9", "av1"];
    const AUDIO: &[&str] = &["mp3", "aac", "opus", "pcm_s16le"];
    const CONTAINER: &[&str] = &["mov", "mp4", "ogg", "webm"];
    const CQ: &[&str] = &["auto", "cqp", "icq"];
    const POLICY: &[&str] = &["all", "optimal", "bitrate", "required", "disabled"];
    const ACCEL: &[&str] = &["nvenc", "qsv", "vaapi", "rkmpp", "disabled"];
    const TONE: &[&str] = &["hable", "mobius", "reinhard", "disabled"];
    const LOG: &[&str] = &["verbose", "debug", "log", "warn", "error", "fatal"];
    const OAUTH: &[&str] = &["client_secret_post", "client_secret_basic"];
    const CHANNEL: &[&str] = &["stable", "releaseCandidate"];

    match path {
        "image.preview.format" | "image.thumbnail.format" | "image.fullsize.format" => {
            Some(IMAGE_FORMAT)
        }
        "image.colorspace" => Some(COLORSPACE),
        "ffmpeg.targetVideoCodec"
        | "ffmpeg.acceptedVideoCodecs"
        | "ffmpeg.realtime.videoCodecs" => Some(VIDEO),
        "ffmpeg.targetAudioCodec" | "ffmpeg.acceptedAudioCodecs" => Some(AUDIO),
        "ffmpeg.acceptedContainers" => Some(CONTAINER),
        "ffmpeg.cqMode" => Some(CQ),
        "ffmpeg.transcode" => Some(POLICY),
        "ffmpeg.accel" => Some(ACCEL),
        "ffmpeg.tonemap" => Some(TONE),
        "logging.level" => Some(LOG),
        "oauth.tokenEndpointAuthMethod" => Some(OAUTH),
        "newVersionCheck.channel" => Some(CHANNEL),
        _ => None,
    }
}

fn empty_or_url_message(path: &str) -> Option<&'static str> {
    match path {
        "oauth.issuerUrl" => Some("Issuer URL must be an empty string or a valid URL"),
        "oauth.accountManagementUrl" => {
            Some("Account management URL must be an empty string or a valid URL")
        }
        "oauth.endSessionEndpoint" => {
            Some("endSessionEndpoint must be an empty string or a valid URL")
        }
        "server.externalDomain" => Some("External domain must be an empty string or a valid URL"),
        _ => None,
    }
}

fn strict_url_field(path: &str) -> bool {
    matches!(path, "map.lightStyle" | "map.darkStyle")
}

fn is_hh_mm(value: &str) -> bool {
    let mut parts = value.split(':');
    let (Some(hour), Some(minute), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    if hour.len() != 2 || minute.len() != 2 {
        return false;
    }
    matches!(
        (hour.parse::<u32>(), minute.parse::<u32>()),
        (Ok(hour), Ok(minute)) if hour <= 23 && minute <= 59
    )
}

fn cron_error(expression: &str) -> Result<(), String> {
    match crate::utils::cron::node_cron_error(expression) {
        Some(message) => Err(format!("Invalid cron expression. {message}")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_transcode_policy_and_bad_cron() {
        let mut config = crate::utils::system_config::defaults();
        config["ffmpeg"]["transcode"] = Value::String("nope".into());
        assert!(normalize_admin_config(&crate::utils::system_config::defaults(), &config).is_err());

        let mut config = crate::utils::system_config::defaults();
        config["backup"]["database"]["cronExpression"] = Value::String("not a cron".into());
        let err =
            normalize_admin_config(&crate::utils::system_config::defaults(), &config).unwrap_err();
        assert!(err.iter().any(|issue| {
            issue.code == "custom" && issue.message == "Invalid cron expression. Unknown alias: not"
        }));
    }

    #[test]
    fn empty_machine_learning_urls_use_the_default() {
        let defaults = crate::utils::system_config::defaults();
        let mut config = defaults.clone();
        config["machineLearning"]["urls"] = Value::Array(Vec::new());
        let normalized = normalize_admin_config(&defaults, &config).unwrap();
        assert_eq!(
            normalized["machineLearning"]["urls"],
            defaults["machineLearning"]["urls"]
        );
        assert!(
            !normalized["machineLearning"]["urls"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn accepts_defaults() {
        let config = crate::utils::system_config::defaults();
        assert!(normalize_admin_config(&config, &config).is_ok());
    }

    #[test]
    fn rejects_missing_required_field_and_coerces_bool_string() {
        let defaults = crate::utils::system_config::defaults();
        let mut config = defaults.clone();
        config.as_object_mut().unwrap().remove("trash");
        let err = normalize_admin_config(&defaults, &config).unwrap_err();
        assert!(err.iter().any(|issue| {
            issue
                .message
                .contains("expected object, received undefined")
                && issue
                    .path
                    .iter()
                    .any(|segment| segment.as_str() == Some("trash"))
        }));

        let mut config = defaults.clone();
        config["trash"]["enabled"] = Value::String("true".into());
        let normalized = normalize_admin_config(&defaults, &config).unwrap();
        assert_eq!(normalized["trash"]["enabled"], Value::Bool(true));

        let mut config = defaults.clone();
        config["ffmpeg"]["crf"] = Value::String("23".into());
        let normalized = normalize_admin_config(&defaults, &config).unwrap();
        assert_eq!(normalized["ffmpeg"]["crf"], serde_json::json!(23));

        let mut config = defaults.clone();
        config["machineLearning"]["clip"]["enabled"] = Value::String("true".into());
        assert!(normalize_admin_config(&defaults, &config).is_err());

        let mut config = defaults.clone();
        config["server"]["externalDomain"] = Value::String("not a url".into());
        let err = normalize_admin_config(&defaults, &config).unwrap_err();
        assert!(err.iter().any(|issue| {
            issue
                .message
                .contains("External domain must be an empty string or a valid URL")
        }));

        let mut config = defaults.clone();
        config["ffmpeg"]["crf"] = Value::String("not-a-number".into());
        let err = normalize_admin_config(&defaults, &config).unwrap_err();
        assert!(err.iter().any(|issue| {
            issue.code == "invalid_type"
                && issue.expected.as_deref() == Some("number")
                && issue.message == "Invalid input: expected number, received NaN"
        }));

        let mut config = defaults;
        config["ffmpeg"]["transcode"] = Value::String("unknown".into());
        let err =
            normalize_admin_config(&crate::utils::system_config::defaults(), &config).unwrap_err();
        assert!(err.iter().any(|issue| {
            issue.message.starts_with("Invalid option: expected one of")
                && issue.message.contains("\"all\"")
        }));
    }
}

fn json_equal(left: Option<&Value>, right: Option<&Value>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}
