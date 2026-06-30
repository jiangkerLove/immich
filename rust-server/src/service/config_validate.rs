use serde_json::Value;

use crate::models::response::response::ErrorResp;
use crate::service::bootstrap;
use crate::service::email::{EmailService, SmtpTransportConfig};
use crate::service::media::storage_template::StorageTemplateService;
use crate::utils::clip::get_clip_dim_size;
pub async fn validate_system_config(
    old_config: &Value,
    new_config: &Value,
) -> Result<(), ErrorResp> {
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

    if env.immich_log_level.is_some()
        && !json_equal(
            old_config.get("logging"),
            new_config.get("logging"),
        )
    {
        return Err(ErrorResp::BadRequest(
            "Logging cannot be changed while the environment variable IMMICH_LOG_LEVEL is set."
                .to_string(),
        ));
    }

    if let Some(model_name) = new_config
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

    if let Some(template) = new_config
        .get("storageTemplate")
        .and_then(|value| value.get("template"))
        .and_then(|value| value.as_str())
    {
        StorageTemplateService::validate_storage_template(template)
            .map_err(|_| ErrorResp::BadRequest("Invalid storage template".to_string()))?;
    }

    let smtp_enabled = new_config
        .get("notifications")
        .and_then(|value| value.get("smtp"))
        .and_then(|value| value.get("enabled"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if smtp_enabled
        && !json_equal(
            old_config
                .get("notifications")
                .and_then(|value| value.get("smtp")),
            new_config
                .get("notifications")
                .and_then(|value| value.get("smtp")),
        )
    {
        let transport = new_config
            .get("notifications")
            .and_then(|value| value.get("smtp"))
            .and_then(|value| value.get("transport"))
            .ok_or_else(|| ErrorResp::BadRequest("Invalid SMTP configuration".to_string()))?;
        let transport: SmtpTransportConfig = serde_json::from_value(transport.clone())
            .map_err(|_| ErrorResp::BadRequest("Invalid SMTP configuration".to_string()))?;
        EmailService::verify_smtp(&transport).await?;
    }

    let _ = env;
    Ok(())
}
fn json_equal(left: Option<&Value>, right: Option<&Value>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}
