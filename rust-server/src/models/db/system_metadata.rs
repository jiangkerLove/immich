use std::collections::HashMap;

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
pub struct FacialRecognitionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_facial_model")]
    pub model_name: String,
    #[serde(default = "default_min_score")]
    pub min_score: f64,
    #[serde(default = "default_max_distance")]
    pub max_distance: f64,
    #[serde(default = "default_min_faces")]
    pub min_faces: i32,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OcrConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ocr_model")]
    pub model_name: String,
    #[serde(default = "default_min_score")]
    pub min_detection_score: f64,
    #[serde(default = "default_min_score")]
    pub min_recognition_score: f64,
    #[serde(default = "default_max_resolution")]
    pub max_resolution: i32,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateDetectionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_duplicate_distance")]
    pub max_distance: f64,
}

fn default_facial_model() -> String {
    "buffalo_l".to_string()
}

fn default_ocr_model() -> String {
    "PP-OCRv5_mobile".to_string()
}

fn default_min_score() -> f64 {
    0.7
}

fn default_max_distance() -> f64 {
    0.5
}

fn default_min_faces() -> i32 {
    3
}

fn default_max_resolution() -> i32 {
    736
}

fn default_duplicate_distance() -> f64 {
    0.01
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
    #[serde(default)]
    pub facial_recognition: FacialRecognitionConfig,
    #[serde(default)]
    pub ocr: OcrConfig,
    #[serde(default)]
    pub duplicate_detection: DuplicateDetectionConfig,
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

pub fn is_ocr_enabled(config: &MachineLearningConfig) -> bool {
    config.enabled && config.ocr.enabled
}

pub fn is_facial_recognition_enabled(config: &MachineLearningConfig) -> bool {
    config.enabled && config.facial_recognition.enabled
}

pub fn is_duplicate_detection_enabled(config: &MachineLearningConfig) -> bool {
    is_smart_search_enabled(config) && config.duplicate_detection.enabled
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

pub async fn set_reverse_geocoding_state(
    pool: &Pool<Postgres>,
    state: &ReverseGeocodingState,
) -> Result<(), sqlx::Error> {
    set_json(
        pool,
        REVERSE_GEOCODING_STATE_KEY,
        &serde_json::to_value(state).unwrap_or_default(),
    )
    .await
}

pub async fn get_version_check_state(
    pool: &Pool<Postgres>,
) -> Result<VersionCheckState, sqlx::Error> {
    let value = get_json(pool, VERSION_CHECK_STATE_KEY).await?;
    Ok(value
        .and_then(|json| serde_json::from_value::<VersionCheckState>(json).ok())
        .unwrap_or_default())
}

const MEMORIES_STATE_KEY: &str = "memories-state";

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemoriesState {
    pub last_on_this_day_date: Option<String>,
}

pub async fn get_memories_state(pool: &Pool<Postgres>) -> Result<MemoriesState, sqlx::Error> {
    let value = get_json(pool, MEMORIES_STATE_KEY).await?;
    Ok(value
        .and_then(|json| serde_json::from_value::<MemoriesState>(json).ok())
        .unwrap_or_default())
}

pub async fn set_memories_state(
    pool: &Pool<Postgres>,
    state: &MemoriesState,
) -> Result<(), sqlx::Error> {
    set_json(
        pool,
        MEMORIES_STATE_KEY,
        &serde_json::to_value(state).unwrap_or_default(),
    )
    .await
}

const FACIAL_RECOGNITION_STATE_KEY: &str = "facial-recognition-state";

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FacialRecognitionState {
    pub last_run: Option<String>,
}

pub async fn get_facial_recognition_state(
    pool: &Pool<Postgres>,
) -> Result<FacialRecognitionState, sqlx::Error> {
    let value = get_json(pool, FACIAL_RECOGNITION_STATE_KEY).await?;
    Ok(value
        .and_then(|json| serde_json::from_value::<FacialRecognitionState>(json).ok())
        .unwrap_or_default())
}

pub async fn set_facial_recognition_state(
    pool: &Pool<Postgres>,
    state: &FacialRecognitionState,
) -> Result<(), sqlx::Error> {
    set_json(
        pool,
        FACIAL_RECOGNITION_STATE_KEY,
        &serde_json::to_value(state).unwrap_or_default(),
    )
    .await
}

pub async fn get_custom_css(pool: &Pool<Postgres>) -> Result<String, sqlx::Error> {
    let json = get_json(pool, "system-config").await?;
    Ok(json
        .and_then(|value| serde_json::from_value::<SystemConfigRoot>(value).ok())
        .map(|cfg| cfg.theme.custom_css)
        .unwrap_or_default())
}

const SYSTEM_FLAGS_KEY: &str = "system-flags";
const MEDIA_LOCATION_KEY: &str = "MediaLocation";

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SystemFlags {
    #[serde(default)]
    pub mount_checks: HashMap<String, bool>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaLocationMeta {
    pub location: String,
}

pub async fn get_system_flags(pool: &Pool<Postgres>) -> Result<Option<SystemFlags>, sqlx::Error> {
    let value = get_json(pool, SYSTEM_FLAGS_KEY).await?;
    Ok(value.and_then(|json| serde_json::from_value::<SystemFlags>(json).ok()))
}

pub async fn set_system_flags(
    pool: &Pool<Postgres>,
    flags: &SystemFlags,
) -> Result<(), sqlx::Error> {
    set_json(
        pool,
        SYSTEM_FLAGS_KEY,
        &serde_json::to_value(flags).unwrap_or_default(),
    )
    .await
}

pub async fn get_media_location(
    pool: &Pool<Postgres>,
) -> Result<Option<MediaLocationMeta>, sqlx::Error> {
    let value = get_json(pool, MEDIA_LOCATION_KEY).await?;
    Ok(value.and_then(|json| serde_json::from_value::<MediaLocationMeta>(json).ok()))
}

pub async fn set_media_location(
    pool: &Pool<Postgres>,
    meta: &MediaLocationMeta,
) -> Result<(), sqlx::Error> {
    set_json(
        pool,
        MEDIA_LOCATION_KEY,
        &serde_json::to_value(meta).unwrap_or_default(),
    )
    .await
}
