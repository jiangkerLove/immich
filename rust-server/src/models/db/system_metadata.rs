use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Postgres};

pub async fn get_json(pool: &Pool<Postgres>, key: &str) -> Result<Option<Value>, sqlx::Error> {
    let value: Option<Value> =
        sqlx::query_scalar(r#"SELECT value FROM system_metadata WHERE key = $1"#)
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(value)
}

pub async fn set_json(pool: &Pool<Postgres>, key: &str, value: &Value) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO system_metadata (key, value)
        VALUES ($1, $2)
        ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

const ADMIN_ONBOARDING_KEY: &str = "admin-onboarding";

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AdminOnboarding {
    pub is_onboarded: bool,
}

pub async fn get_admin_onboarding(pool: &Pool<Postgres>) -> Result<AdminOnboarding, sqlx::Error> {
    let value = get_json(pool, ADMIN_ONBOARDING_KEY).await?;
    Ok(value
        .and_then(|json| serde_json::from_value::<AdminOnboarding>(json).ok())
        .unwrap_or_default())
}

pub async fn set_admin_onboarding(
    pool: &Pool<Postgres>,
    onboarding: &AdminOnboarding,
) -> Result<(), sqlx::Error> {
    set_json(
        pool,
        ADMIN_ONBOARDING_KEY,
        &serde_json::to_value(onboarding).unwrap_or_default(),
    )
    .await
}

const LICENSE_KEY: &str = "license";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ServerLicense {
    pub license_key: String,
    pub activation_key: String,
    pub activated_at: String,
}

pub async fn get_server_license(pool: &Pool<Postgres>) -> Result<Option<ServerLicense>, sqlx::Error> {
    let value = get_json(pool, LICENSE_KEY).await?;
    Ok(value.and_then(|json| serde_json::from_value::<ServerLicense>(json).ok()))
}

pub async fn set_server_license(
    pool: &Pool<Postgres>,
    license: &ServerLicense,
) -> Result<(), sqlx::Error> {
    set_json(pool, LICENSE_KEY, &serde_json::to_value(license).unwrap_or_default()).await
}

pub async fn delete_server_license(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM system_metadata WHERE key = $1"#)
        .bind(LICENSE_KEY)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OAuthConfig {
    pub enabled: bool,
    pub auto_register: bool,
    pub auto_launch: bool,
    pub button_text: String,
    pub client_id: String,
    pub client_secret: String,
    pub issuer_url: String,
    pub end_session_endpoint: String,
    pub mobile_override_enabled: bool,
    pub mobile_redirect_uri: String,
    pub prompt: String,
    pub scope: String,
    pub signing_algorithm: String,
    pub profile_signing_algorithm: String,
    pub token_endpoint_auth_method: String,
    pub timeout: u64,
    pub allow_insecure_requests: bool,
    pub default_storage_quota: Option<i64>,
    pub storage_label_claim: String,
    pub storage_quota_claim: String,
    pub role_claim: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ClipConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub model_name: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MachineLearningConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub clip: ClipConfig,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ThemeConfig {
    #[serde(default)]
    pub custom_css: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemConfigRoot {
    pub oauth: OAuthConfig,
    #[serde(default)]
    pub password_login: PasswordLoginConfig,
    #[serde(default)]
    pub machine_learning: MachineLearningConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PasswordLoginConfig {
    pub enabled: bool,
}

pub async fn get_oauth_config(pool: &Pool<Postgres>) -> Result<Option<OAuthConfig>, sqlx::Error> {
    let json = get_json(pool, "system-config").await?;
    Ok(json.and_then(|value| {
        serde_json::from_value::<SystemConfigRoot>(value)
            .ok()
            .map(|cfg| cfg.oauth)
    }))
}

pub async fn password_login_enabled(pool: &Pool<Postgres>) -> Result<bool, sqlx::Error> {
    let json = get_json(pool, "system-config").await?;
    Ok(json
        .and_then(|value| serde_json::from_value::<SystemConfigRoot>(value).ok())
        .map(|cfg| cfg.password_login.enabled)
        .unwrap_or(true))
}

pub async fn get_machine_learning_config(
    pool: &Pool<Postgres>,
) -> Result<MachineLearningConfig, sqlx::Error> {
    let json = get_json(pool, "system-config").await?;
    Ok(json
        .and_then(|value| serde_json::from_value::<SystemConfigRoot>(value).ok())
        .map(|cfg| cfg.machine_learning)
        .unwrap_or_default())
}

pub fn is_smart_search_enabled(config: &MachineLearningConfig) -> bool {
    config.enabled && config.clip.enabled
}

const REVERSE_GEOCODING_STATE_KEY: &str = "reverse-geocoding-state";
const VERSION_CHECK_STATE_KEY: &str = "version-check-state";

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReverseGeocodingState {
    pub last_update: Option<String>,
    pub last_import_file_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionCheckState {
    pub checked_at: Option<String>,
    pub release_version: Option<String>,
}

pub async fn get_reverse_geocoding_state(
    pool: &Pool<Postgres>,
) -> Result<ReverseGeocodingState, sqlx::Error> {
    let value = get_json(pool, REVERSE_GEOCODING_STATE_KEY).await?;
    Ok(value
        .and_then(|json| serde_json::from_value::<ReverseGeocodingState>(json).ok())
        .unwrap_or_default())
}

pub async fn get_version_check_state(
    pool: &Pool<Postgres>,
) -> Result<VersionCheckState, sqlx::Error> {
    let value = get_json(pool, VERSION_CHECK_STATE_KEY).await?;
    Ok(value
        .and_then(|json| serde_json::from_value::<VersionCheckState>(json).ok())
        .unwrap_or_default())
}

pub async fn get_custom_css(pool: &Pool<Postgres>) -> Result<String, sqlx::Error> {
    let json = get_json(pool, "system-config").await?;
    Ok(json
        .and_then(|value| serde_json::from_value::<SystemConfigRoot>(value).ok())
        .map(|cfg| cfg.theme.custom_css)
        .unwrap_or_default())
}
