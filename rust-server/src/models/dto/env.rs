use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "SCREAMING_SNAKE_CASE")]
pub struct EnvDto {
    pub immich_api_metrics_port: Option<u16>,
    pub immich_build_data: Option<String>,
    pub immich_build: Option<String>,
    pub immich_build_url: Option<String>,
    pub immich_build_image: Option<String>,
    pub immich_build_image_url: Option<String>,
    pub immich_config_file: Option<String>,
    pub immich_env: Option<ImmichEnvironment>,
    pub immich_host: Option<String>,
    pub immich_ignore_mount_check_errors: Option<bool>,
    pub immich_log_level: Option<LogLevel>,
    pub immich_microservices_metrics_port: Option<u16>,
    pub immich_port: Option<u16>,
    pub immich_repository: Option<String>,
    pub immich_repository_url: Option<String>,
    pub immich_source_ref: Option<String>,
    pub immich_source_commit: Option<String>,
    pub immich_source_url: Option<String>,
    pub immich_telemetry_include: Option<String>,
    pub immich_telemetry_exclude: Option<String>,
    pub immich_third_party_source_url: Option<String>,
    pub immich_third_party_bug_feature_url: Option<String>,
    pub immich_third_party_documentation_url: Option<String>,
    pub immich_third_party_support_url: Option<String>,
    pub immich_trusted_proxies: Option<String>, // 可以自定义解析为 Vec<String>
    pub immich_workers_include: Option<String>,
    pub immich_workers_exclude: Option<String>,

    pub db_database_name: String,
    pub db_hostname: String,
    pub db_password: String,
    pub db_port: u16,
    pub db_ssl_mode: Option<DatabaseSslMode>,
    pub db_url: String,
    pub db_username: String,
    pub db_vector_extension: Option<DbVectorExtension>,

    pub no_color: Option<String>,

    pub redis_hostname: String,
    pub redis_port: u16,
    pub redis_dbindex: u8,
    pub redis_username: Option<String>,
    pub redis_password: Option<String>,
    pub redis_socket: Option<String>,
    pub redis_url: Option<String>,

    pub upload_location: Option<String>,
    pub immich_media_location: Option<String>,
}

impl Default for EnvDto {
    fn default() -> Self {
        Self {
            immich_api_metrics_port: None,
            immich_build_data: None,
            immich_build: None,
            immich_build_url: None,
            immich_build_image: None,
            immich_build_image_url: None,
            immich_config_file: None,
            immich_env: None,
            immich_host: None,
            immich_ignore_mount_check_errors: None,
            immich_log_level: None,
            immich_microservices_metrics_port: None,
            immich_port: None,
            immich_repository: None,
            immich_repository_url: None,
            immich_source_ref: None,
            immich_source_commit: None,
            immich_source_url: None,
            immich_telemetry_include: None,
            immich_telemetry_exclude: None,
            immich_third_party_source_url: None,
            immich_third_party_bug_feature_url: None,
            immich_third_party_documentation_url: None,
            immich_third_party_support_url: None,
            immich_trusted_proxies: None,
            immich_workers_include: None,
            immich_workers_exclude: None,
            db_database_name: "immich".into(),
            db_hostname: "database".into(),
            db_password: "postgres".into(),
            db_port: 5432,
            db_ssl_mode: None,
            db_url: "localhost".into(),
            db_username: "postgres".into(),
            db_vector_extension: None,
            no_color: None,
            redis_hostname: "redis".into(),
            redis_port: 6379,
            redis_dbindex: 0,
            redis_username: None,
            redis_password: None,
            redis_socket: None,
            redis_url: None,
            upload_location: None,
            immich_media_location: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImmichEnvironment {
    Development,
    Production,
    Test,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseSslMode {
    Disable,
    Require,
    VerifyCa,
    VerifyFull,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DbVectorExtension {
    #[serde(rename = "pgvector")]
    PgVector,
    #[serde(rename = "pgvecto.rs")]
    PgvectoRs,
    #[serde(rename = "vectorchord")]
    VectorChord,
}
