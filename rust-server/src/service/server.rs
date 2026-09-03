use std::path::PathBuf;

use axum::body::Body;
use axum::http::Response;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::OnceCell;

use crate::constants::SERVER_VERSION;
use crate::models::db::system_metadata;
use crate::models::db::version_history;
use crate::models::dto::env::EnvDto;
use crate::models::response::response::ErrorResp;
use crate::service::hls::is_maintenance_mode;
use crate::utils::bytes::as_human_readable;
use crate::utils::disk::check_disk_usage;
use crate::utils::mime_types::{
    supported_image_extensions, supported_sidecar_extensions, supported_video_extensions,
};
use crate::utils::response::json_response;
use crate::utils::system_config::{
    get_merged, is_duplicate_detection_enabled, is_facial_recognition_enabled, is_ocr_enabled,
    is_smart_search_enabled, json_bool, json_i32, json_str,
};

#[derive(Clone)]
pub struct ServerBuildConfig {
    pub build: Option<String>,
    pub build_url: Option<String>,
    pub build_image: Option<String>,
    pub build_image_url: Option<String>,
    pub repository: Option<String>,
    pub repository_url: Option<String>,
    pub source_ref: Option<String>,
    pub source_commit: Option<String>,
    pub source_url: Option<String>,
    pub third_party_source_url: Option<String>,
    pub third_party_bug_feature_url: Option<String>,
    pub third_party_documentation_url: Option<String>,
    pub third_party_support_url: Option<String>,
}

impl ServerBuildConfig {
    pub fn from_env(env: &EnvDto) -> Self {
        Self {
            build: env.immich_build.clone(),
            build_url: env.immich_build_url.clone(),
            build_image: env.immich_build_image.clone(),
            build_image_url: env.immich_build_image_url.clone(),
            repository: env
                .immich_repository
                .clone()
                .or_else(|| Some("immich-app/immich".to_string())),
            repository_url: env
                .immich_repository_url
                .clone()
                .or_else(|| Some("https://github.com/immich-app/immich".to_string())),
            source_ref: env.immich_source_ref.clone(),
            source_commit: env.immich_source_commit.clone(),
            source_url: env.immich_source_url.clone(),
            third_party_source_url: env.immich_third_party_source_url.clone(),
            third_party_bug_feature_url: env.immich_third_party_bug_feature_url.clone(),
            third_party_documentation_url: env.immich_third_party_documentation_url.clone(),
            third_party_support_url: env.immich_third_party_support_url.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ServerService {
    pool: PgPool,
    build_config: ServerBuildConfig,
    tool_versions: std::sync::Arc<OnceCell<ToolVersions>>,
    library_path: PathBuf,
    config_file: bool,
    allow_setup: bool,
}

#[derive(Clone, Default)]
struct ToolVersions {
    ffmpeg: String,
    imagemagick: String,
    libvips: String,
    exiftool: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerAboutResponse {
    pub version: String,
    pub version_url: String,
    pub licensed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ffmpeg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imagemagick: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub libvips: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exiftool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub third_party_source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub third_party_bug_feature_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub third_party_documentation_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub third_party_support_url: Option<String>,
}

#[derive(Serialize)]
pub struct ServerPingResponse {
    pub res: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerVersionResponse {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerVersionHistoryResponse {
    pub id: uuid::Uuid,
    pub created_at: String,
    pub version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerFeaturesResponse {
    pub smart_search: bool,
    pub facial_recognition: bool,
    pub duplicate_detection: bool,
    pub map: bool,
    pub reverse_geocoding: bool,
    pub import_faces: bool,
    pub sidecar: bool,
    pub search: bool,
    pub trash: bool,
    pub oauth: bool,
    pub oauth_auto_launch: bool,
    pub ocr: bool,
    pub password_login: bool,
    pub config_file: bool,
    pub email: bool,
    pub realtime_transcoding: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfigResponse {
    pub login_page_message: String,
    pub trash_days: i32,
    pub user_delete_delay: i32,
    pub oauth_button_text: String,
    pub oauth_account_management_url: String,
    pub is_initialized: bool,
    pub is_onboarded: bool,
    pub external_domain: String,
    pub public_users: bool,
    pub map_dark_style_url: String,
    pub map_light_style_url: String,
    pub maintenance_mode: bool,
    pub min_faces: i32,
}

#[derive(Serialize)]
pub struct ServerMediaTypesResponse {
    pub image: Vec<String>,
    pub video: Vec<String>,
    pub sidecar: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStorageResponse {
    pub disk_size: String,
    pub disk_use: String,
    pub disk_available: String,
    pub disk_size_raw: i64,
    pub disk_use_raw: i64,
    pub disk_available_raw: i64,
    pub disk_usage_percentage: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageByUserResponse {
    pub user_id: uuid::Uuid,
    pub user_name: String,
    pub photos: i64,
    pub videos: i64,
    pub usage: i64,
    pub usage_photos: i64,
    pub usage_videos: i64,
    pub quota_size_in_bytes: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatsResponse {
    pub photos: i64,
    pub videos: i64,
    pub usage: i64,
    pub usage_photos: i64,
    pub usage_videos: i64,
    pub usage_by_user: Vec<UsageByUserResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerApkLinksResponse {
    pub arm64v8a: String,
    pub armeabiv7a: String,
    pub universal: String,
    pub x86_64: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseKeyReq {
    pub license_key: String,
    pub activation_key: String,
}

#[derive(Serialize)]
pub struct WellKnownResponse {
    pub api: WellKnownApi,
}

#[derive(Serialize)]
pub struct WellKnownApi {
    pub endpoint: String,
}

impl ServerService {
    pub fn new(
        pool: PgPool,
        build_config: ServerBuildConfig,
        library_path: PathBuf,
        config_file: bool,
        allow_setup: bool,
    ) -> Self {
        Self {
            pool,
            build_config,
            tool_versions: std::sync::Arc::new(OnceCell::new()),
            library_path,
            config_file,
            allow_setup,
        }
    }

    pub fn ping() -> ServerPingResponse {
        ServerPingResponse {
            res: "pong".to_string(),
        }
    }

    pub fn version() -> ServerVersionResponse {
        let mut parts = SERVER_VERSION.split('.');
        let major = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let minor = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let patch_part = parts.next().unwrap_or("0");
        let patch = patch_part
            .split('-')
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        ServerVersionResponse {
            major,
            minor,
            patch,
            prerelease: None,
        }
    }

    pub async fn get_version_history(
        &self,
    ) -> Result<Vec<ServerVersionHistoryResponse>, ErrorResp> {
        let rows = version_history::get_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| ServerVersionHistoryResponse {
                id: row.id,
                created_at: row
                    .created_at
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                version: row.version,
            })
            .collect())
    }

    pub async fn get_features(&self) -> Result<ServerFeaturesResponse, ErrorResp> {
        let config = get_merged(&self.pool).await.map_err(ErrorResp::from)?;
        let ml = config.get("machineLearning").cloned().unwrap_or_default();

        Ok(ServerFeaturesResponse {
            smart_search: is_smart_search_enabled(&ml),
            facial_recognition: is_facial_recognition_enabled(&ml),
            duplicate_detection: is_duplicate_detection_enabled(&ml),
            map: json_bool(&config, &["map", "enabled"], true),
            reverse_geocoding: json_bool(&config, &["reverseGeocoding", "enabled"], true),
            import_faces: json_bool(&config, &["metadata", "faces", "import"], false),
            sidecar: true,
            search: true,
            trash: json_bool(&config, &["trash", "enabled"], true),
            oauth: json_bool(&config, &["oauth", "enabled"], false),
            oauth_auto_launch: json_bool(&config, &["oauth", "autoLaunch"], false),
            ocr: is_ocr_enabled(&ml),
            password_login: json_bool(&config, &["passwordLogin", "enabled"], true),
            config_file: self.config_file,
            email: json_bool(&config, &["notifications", "smtp", "enabled"], false),
            realtime_transcoding: json_bool(&config, &["ffmpeg", "realtime", "enabled"], false),
        })
    }

    pub async fn get_config(&self) -> Result<ServerConfigResponse, ErrorResp> {
        let config = get_merged(&self.pool).await.map_err(ErrorResp::from)?;

        let has_admin: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "user" WHERE "isAdmin" = true AND "deletedAt" IS NULL)"#,
        )
        .fetch_one(&self.pool)
        .await?;

        let admin_onboarding = system_metadata::get_admin_onboarding(&self.pool).await?;
        let maintenance_mode = is_maintenance_mode(&self.pool).await?;

        Ok(ServerConfigResponse {
            login_page_message: json_str(&config, &["server", "loginPageMessage"], ""),
            trash_days: json_i32(&config, &["trash", "days"], 30),
            user_delete_delay: json_i32(&config, &["user", "deleteDelay"], 7),
            oauth_button_text: json_str(&config, &["oauth", "buttonText"], "Login with OAuth"),
            oauth_account_management_url: json_str(&config, &["oauth", "accountManagementUrl"], ""),
            is_initialized: !self.allow_setup || has_admin,
            is_onboarded: admin_onboarding.is_onboarded,
            external_domain: json_str(&config, &["server", "externalDomain"], ""),
            public_users: json_bool(&config, &["server", "publicUsers"], true),
            map_dark_style_url: json_str(
                &config,
                &["map", "darkStyle"],
                "https://tiles.immich.cloud/v1/style/dark.json",
            ),
            map_light_style_url: json_str(
                &config,
                &["map", "lightStyle"],
                "https://tiles.immich.cloud/v1/style/light.json",
            ),
            maintenance_mode,
            min_faces: json_i32(
                &config,
                &["machineLearning", "facialRecognition", "minFaces"],
                3,
            ),
        })
    }

    pub async fn get_about(&self) -> Result<ServerAboutResponse, ErrorResp> {
        let version = format!("v{SERVER_VERSION}");
        let version_url = format!("https://github.com/immich-app/immich/releases/tag/{version}");

        let license = system_metadata::get_json(&self.pool, "license").await?;
        let licensed = license.is_some();

        let tools = self.get_tool_versions().await;
        let cfg = &self.build_config;

        Ok(ServerAboutResponse {
            version,
            version_url,
            licensed,
            repository: cfg.repository.clone(),
            repository_url: cfg.repository_url.clone(),
            source_ref: cfg.source_ref.clone(),
            source_commit: cfg.source_commit.clone(),
            source_url: cfg.source_url.clone(),
            build: cfg.build.clone(),
            build_url: cfg.build_url.clone(),
            build_image: cfg.build_image.clone(),
            build_image_url: cfg.build_image_url.clone(),
            ffmpeg: non_empty(tools.ffmpeg),
            imagemagick: non_empty(tools.imagemagick),
            libvips: non_empty(tools.libvips),
            exiftool: non_empty(tools.exiftool),
            third_party_source_url: cfg.third_party_source_url.clone(),
            third_party_bug_feature_url: cfg.third_party_bug_feature_url.clone(),
            third_party_documentation_url: cfg.third_party_documentation_url.clone(),
            third_party_support_url: cfg.third_party_support_url.clone(),
        })
    }

    pub async fn get_custom_css(&self) -> Result<String, ErrorResp> {
        system_metadata::get_custom_css(&self.pool)
            .await
            .map_err(ErrorResp::from)
    }

    async fn get_tool_versions(&self) -> ToolVersions {
        self.tool_versions
            .get_or_init(probe_tool_versions)
            .await
            .clone()
    }

    pub fn get_storage(
        &self,
        auth: &crate::models::dto::auth::AuthDto,
    ) -> Result<ServerStorageResponse, ErrorResp> {
        if !auth.user.is_admin {
            return Err(ErrorResp::Forbidden("Forbidden".to_string()));
        }

        let disk = check_disk_usage(&self.library_path)
            .ok_or_else(|| ErrorResp::ServerError("Failed to read disk usage".to_string()))?;

        let disk_use_raw = disk.used;
        let usage_percentage = if disk.total == 0 {
            0.0
        } else {
            ((disk_use_raw as f64 / disk.total as f64) * 10000.0).round() / 100.0
        };

        Ok(ServerStorageResponse {
            disk_size: as_human_readable(disk.total, 1),
            disk_use: as_human_readable(disk_use_raw, 1),
            disk_available: as_human_readable(disk.available, 1),
            disk_size_raw: disk.total as i64,
            disk_use_raw: disk_use_raw as i64,
            disk_available_raw: disk.available as i64,
            disk_usage_percentage: usage_percentage,
        })
    }

    pub async fn get_statistics(
        &self,
        auth: &crate::models::dto::auth::AuthDto,
    ) -> Result<ServerStatsResponse, ErrorResp> {
        if !auth.user.is_admin {
            return Err(ErrorResp::Forbidden("Forbidden".to_string()));
        }

        let rows = crate::models::db::users::get_user_stats(&self.pool).await?;
        let mut stats = ServerStatsResponse {
            photos: 0,
            videos: 0,
            usage: 0,
            usage_photos: 0,
            usage_videos: 0,
            usage_by_user: Vec::with_capacity(rows.len()),
        };

        for row in rows {
            stats.photos += row.photos;
            stats.videos += row.videos;
            stats.usage += row.usage;
            stats.usage_photos += row.usage_photos;
            stats.usage_videos += row.usage_videos;
            stats.usage_by_user.push(UsageByUserResponse {
                user_id: row.user_id,
                user_name: row.user_name,
                photos: row.photos,
                videos: row.videos,
                usage: row.usage,
                usage_photos: row.usage_photos,
                usage_videos: row.usage_videos,
                quota_size_in_bytes: row.quota_size_in_bytes,
            });
        }

        Ok(stats)
    }

    pub fn get_apk_links(&self) -> ServerApkLinksResponse {
        let base_url =
            format!("https://github.com/immich-app/immich/releases/download/v{SERVER_VERSION}");
        ServerApkLinksResponse {
            arm64v8a: format!("{base_url}/app-arm64-v8a-release.apk"),
            armeabiv7a: format!("{base_url}/app-armeabi-v7a-release.apk"),
            universal: format!("{base_url}/app-release.apk"),
            x86_64: format!("{base_url}/app-x86_64-release.apk"),
        }
    }

    pub async fn get_license(
        &self,
        auth: &crate::models::dto::auth::AuthDto,
    ) -> Result<crate::models::response::user::UserLicenseResponse, ErrorResp> {
        use crate::models::db::auth_permission::Permission;
        use crate::utils::permission::{require_admin, require_permission};

        require_permission(auth, Permission::ServerLicenseRead)?;
        require_admin(auth)?;

        let license = system_metadata::get_server_license(&self.pool)
            .await
            .map_err(ErrorResp::from)?
            .ok_or_else(|| ErrorResp::NotFound("License not found".to_string()))?;

        let activated_at = chrono::DateTime::parse_from_rfc3339(&license.activated_at)
            .map(|value| value.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        Ok(crate::models::response::user::UserLicenseResponse {
            license_key: license.license_key,
            activation_key: license.activation_key,
            activated_at,
        })
    }

    pub async fn set_license(
        &self,
        auth: &crate::models::dto::auth::AuthDto,
        dto: &LicenseKeyReq,
    ) -> Result<crate::models::response::user::UserLicenseResponse, ErrorResp> {
        use crate::models::db::auth_permission::Permission;
        use crate::utils::license::{is_valid_server_license_prefix, verify_server_license};
        use crate::utils::permission::{require_admin, require_permission};

        require_permission(auth, Permission::ServerLicenseUpdate)?;
        require_admin(auth)?;

        if !is_valid_server_license_prefix(&dto.license_key)
            || !verify_server_license(&dto.license_key, &dto.activation_key)
        {
            return Err(ErrorResp::BadRequest("Invalid license key".to_string()));
        }

        let activated_at = chrono::Utc::now();
        system_metadata::set_server_license(
            &self.pool,
            &system_metadata::ServerLicense {
                license_key: dto.license_key.clone(),
                activation_key: dto.activation_key.clone(),
                activated_at: activated_at.to_rfc3339(),
            },
        )
        .await
        .map_err(ErrorResp::from)?;

        Ok(crate::models::response::user::UserLicenseResponse {
            license_key: dto.license_key.clone(),
            activation_key: dto.activation_key.clone(),
            activated_at,
        })
    }

    pub async fn delete_license(
        &self,
        auth: &crate::models::dto::auth::AuthDto,
    ) -> Result<(), ErrorResp> {
        use crate::models::db::auth_permission::Permission;
        use crate::utils::permission::{require_admin, require_permission};

        require_permission(auth, Permission::ServerLicenseDelete)?;
        require_admin(auth)?;
        system_metadata::delete_server_license(&self.pool)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn get_version_check(
        &self,
        auth: &crate::models::dto::auth::AuthDto,
    ) -> Result<system_metadata::VersionCheckState, ErrorResp> {
        use crate::models::db::auth_permission::Permission;
        use crate::utils::permission::require_permission;

        require_permission(auth, Permission::ServerVersionCheck)?;
        system_metadata::get_version_check_state(&self.pool)
            .await
            .map_err(ErrorResp::from)
    }

    pub fn get_media_types() -> ServerMediaTypesResponse {
        ServerMediaTypesResponse {
            image: supported_image_extensions(),
            video: supported_video_extensions(),
            sidecar: supported_sidecar_extensions(),
        }
    }
}

fn non_empty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

async fn probe_tool_versions() -> ToolVersions {
    ToolVersions {
        ffmpeg: probe_ffmpeg().await,
        imagemagick: probe_imagemagick().await,
        libvips: probe_libvips().await,
        exiftool: probe_exiftool().await,
    }
}

async fn command_first_line(program: &str, args: &[&str]) -> String {
    tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .unwrap_or_default()
}

async fn probe_ffmpeg() -> String {
    let line = command_first_line("ffmpeg", &["-version"]).await;
    line.strip_prefix("ffmpeg version ")
        .unwrap_or(&line)
        .to_string()
}

async fn probe_imagemagick() -> String {
    let line = command_first_line("magick", &["--version"]).await;
    if line.is_empty() {
        return String::new();
    }
    line.strip_prefix("Version: ImageMagick ")
        .unwrap_or(&line)
        .to_string()
}

async fn probe_libvips() -> String {
    command_first_line("vips", &["--version"]).await
}

async fn probe_exiftool() -> String {
    command_first_line("exiftool", &["-ver"]).await
}

impl IntoResponse for ServerPingResponse {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}

impl IntoResponse for ServerVersionResponse {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}

impl IntoResponse for ServerFeaturesResponse {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}

impl IntoResponse for ServerConfigResponse {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}

impl IntoResponse for ServerMediaTypesResponse {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}

impl IntoResponse for ServerAboutResponse {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}

impl IntoResponse for ServerStorageResponse {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}

impl IntoResponse for WellKnownResponse {
    fn into_response(self) -> Response<Body> {
        json_response(&self)
    }
}
