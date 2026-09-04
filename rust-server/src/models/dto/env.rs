use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "SCREAMING_SNAKE_CASE")]
pub struct EnvDto {
    pub immich_api_metrics_port: Option<u16>,
    pub immich_build_data: Option<String>,
    pub immich_build: Option<String>,
    pub immich_build_url: Option<String>,
    pub immich_build_image: Option<String>,
    pub immich_build_image_url: Option<String>,
    pub immich_config_file: Option<String>,
    pub immich_allow_setup: Option<bool>,
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
    pub immich_web_root: Option<String>,
    pub immich_third_party_source_url: Option<String>,
    pub immich_third_party_bug_feature_url: Option<String>,
    pub immich_third_party_documentation_url: Option<String>,
    pub immich_third_party_support_url: Option<String>,
    pub immich_trusted_proxies: Option<String>, // 可以自定义解析为 Vec<String>
    pub immich_workers_include: Option<String>,
    pub immich_workers_exclude: Option<String>,

    pub db_database_name: String,
    /// Immich compose default host is `database` (Docker service name).
    pub db_hostname: String,
    pub db_password: String,
    pub db_port: u16,
    pub db_ssl_mode: Option<DatabaseSslMode>,
    /// Immich semantics: full `postgres://…` URL **or** empty.
    /// This fork also accepts a bare hostname here for back-compat.
    pub db_url: String,
    pub db_username: String,
    pub db_vector_extension: Option<DbVectorExtension>,
    pub db_skip_migrations: Option<bool>,

    pub immich_server_path: Option<String>,

    pub redis_hostname: String,
    pub redis_port: u16,
    pub redis_dbindex: u8,
    pub redis_username: Option<String>,
    pub redis_password: Option<String>,
    pub redis_socket: Option<String>,
    pub redis_url: Option<String>,

    pub upload_location: Option<String>,
    pub immich_media_location: Option<String>,
    pub immich_core_plugin: Option<String>,
    pub immich_allow_external_plugins: Option<bool>,
    pub immich_plugins_install_folder: Option<String>,

    pub no_color: Option<String>,
}

impl EnvDto {
    /// Host for `psql`/`pg_dump` and for building a parts-style URL.
    /// Matches Immich: prefer `DB_HOSTNAME` (default `database`); bare `DB_URL` is a legacy host alias.
    pub fn database_host(&self) -> &str {
        let url = self.db_url.trim();
        if url.is_empty() {
            return self.db_hostname.as_str();
        }
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            return self.db_hostname.as_str();
        }
        // Bare host previously documented as DB_URL=192.168.x.x
        url
    }

    /// Connection string for sqlx. Immich: `DB_URL` as full URL overrides parts.
    pub fn postgres_connection_string(&self) -> String {
        let url = self.db_url.trim();
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            return url.to_string();
        }
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.db_username,
            self.db_password,
            self.database_host(),
            self.db_port,
            self.db_database_name,
        )
    }
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
            immich_allow_setup: None,
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
            immich_web_root: None,
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
            // Empty → use DB_HOSTNAME (Immich compose: `database`)
            db_url: String::new(),
            db_username: "postgres".into(),
            db_vector_extension: None,
            db_skip_migrations: None,
            immich_server_path: None,
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
            immich_core_plugin: None,
            immich_allow_external_plugins: None,
            immich_plugins_install_folder: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImmichEnvironment {
    Development,
    Production,
    Test,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseSslMode {
    Disable,
    Require,
    VerifyCa,
    VerifyFull,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DbVectorExtension {
    #[serde(rename = "pgvector")]
    PgVector,
    #[serde(rename = "pgvecto.rs")]
    PgvectoRs,
    #[serde(rename = "vectorchord")]
    VectorChord,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_default_uses_database_hostname() {
        let env = EnvDto::default();
        assert_eq!(env.database_host(), "database");
        assert!(env.postgres_connection_string().contains("@database:5432/"));
    }

    #[test]
    fn bare_db_url_overrides_host() {
        let mut env = EnvDto::default();
        env.db_url = "192.168.1.10".into();
        assert_eq!(env.database_host(), "192.168.1.10");
        assert!(
            env.postgres_connection_string()
                .contains("@192.168.1.10:5432/")
        );
    }

    #[test]
    fn full_db_url_used_as_connection_string() {
        let mut env = EnvDto::default();
        env.db_url = "postgres://u:p@dbhost:5433/immich".into();
        assert_eq!(
            env.postgres_connection_string(),
            "postgres://u:p@dbhost:5433/immich"
        );
    }
}
