use std::path::Path;

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::models::db::system_metadata::{get_json, set_json};
use crate::models::dto::maintenance::{
    MaintenanceAction, MaintenanceAuthResp, MaintenanceDetectInstallFolderResp,
    MaintenanceDetectInstallResp, MaintenanceLoginReq, MaintenanceModeState,
    MaintenanceStatusResp, SetMaintenanceModeReq,
};
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::websocket::WebSocketHub;
use crate::utils::permission::require_admin;
use crate::utils::storage::StoragePaths;

const MAINTENANCE_MODE_KEY: &str = "maintenance-mode";

const STORAGE_FOLDERS: &[&str] = &[
    "encoded-video",
    "library",
    "upload",
    "profile",
    "thumbs",
    "backups",
];

#[derive(Debug, Serialize, Deserialize)]
struct MaintenanceClaims {
    username: String,
    exp: usize,
    iat: usize,
}

#[derive(Clone)]
pub struct MaintenanceService {
    pool: PgPool,
    storage: StoragePaths,
    websocket: WebSocketHub,
}

impl MaintenanceService {
    pub fn new(pool: PgPool, storage: StoragePaths, websocket: WebSocketHub) -> Self {
        Self {
            pool,
            storage,
            websocket,
        }
    }

    pub async fn is_maintenance_mode(&self) -> Result<bool, ErrorResp> {
        Ok(self.get_maintenance_mode().await?.is_maintenance_mode)
    }

    pub async fn get_maintenance_status(&self) -> Result<MaintenanceStatusResp, ErrorResp> {
        let state = self.get_maintenance_mode().await?;
        if state.is_maintenance_mode {
            let action = state
                .action
                .as_ref()
                .map(|a| a.action)
                .unwrap_or(MaintenanceAction::Start);
            Ok(MaintenanceStatusResp {
                active: true,
                action,
                progress: None,
                task: None,
                error: None,
            })
        } else {
            Ok(MaintenanceStatusResp {
                active: false,
                action: MaintenanceAction::End,
                progress: None,
                task: None,
                error: None,
            })
        }
    }

    pub async fn maintenance_login(
        &self,
        dto: &MaintenanceLoginReq,
    ) -> Result<MaintenanceAuthResp, ErrorResp> {
        let state = self.get_maintenance_mode().await?;
        if !state.is_maintenance_mode {
            return Err(ErrorResp::BadRequest("Not in maintenance mode".to_string()));
        }

        let secret = state
            .secret
            .ok_or_else(|| ErrorResp::ServerError("Maintenance secret missing".to_string()))?;

        let token = dto
            .token
            .as_deref()
            .ok_or_else(|| ErrorResp::Unauthorized("Missing JWT Token".to_string()))?;

        let claims = decode_maintenance_jwt(token, &secret)?;
        Ok(MaintenanceAuthResp {
            username: claims.username,
        })
    }

    pub async fn detect_prior_install(&self, auth: &AuthDto) -> Result<MaintenanceDetectInstallResp, ErrorResp> {
        require_admin(auth)?;
        let media = self.storage.media_location();
        let mut storage = Vec::with_capacity(STORAGE_FOLDERS.len());

        for folder in STORAGE_FOLDERS {
            let folder_path = media.join(folder);
            let marker = folder_path.join(".immich");
            let (readable, writable) = check_marker_access(&marker).await;
            let files = count_files(&folder_path).await;

            storage.push(MaintenanceDetectInstallFolderResp {
                folder: (*folder).to_string(),
                readable,
                writable,
                files,
            });
        }

        Ok(MaintenanceDetectInstallResp { storage })
    }

    pub async fn set_maintenance_mode(
        &self,
        auth: &AuthDto,
        dto: &SetMaintenanceModeReq,
    ) -> Result<String, ErrorResp> {
        require_admin(auth)?;

        if dto.action == MaintenanceAction::End {
            return Ok(String::new());
        }

        if dto.action == MaintenanceAction::RestoreDatabase
            && dto
                .restore_backup_filename
                .as_ref()
                .map_or(true, |name| name.is_empty())
        {
            return Err(ErrorResp::BadRequest(
                "Backup filename is required when action is restore_database".to_string(),
            ));
        }

        self.enter_maintenance_mode(dto, &auth.user.name).await
    }

    pub async fn start_restore_flow(&self, is_secure: bool) -> Result<axum::http::Response<axum::body::Body>, ErrorResp> {
        use crate::models::db::users::UserDb;

        if UserDb::get_admin(&self.pool)
            .await
            .map_err(ErrorResp::from)?
            .is_some()
        {
            return Err(ErrorResp::BadRequest(
                "The server already has an admin".to_string(),
            ));
        }

        let dto = SetMaintenanceModeReq {
            action: MaintenanceAction::SelectDatabaseRestore,
            restore_backup_filename: None,
        };
        let jwt = self.enter_maintenance_mode(&dto, "admin").await?;
        Ok(crate::utils::response::respond_with_maintenance_cookie(
            is_secure,
            &jwt,
        ))
    }

    async fn enter_maintenance_mode(
        &self,
        dto: &SetMaintenanceModeReq,
        username: &str,
    ) -> Result<String, ErrorResp> {
        let secret = generate_maintenance_secret();
        let state = MaintenanceModeState {
            is_maintenance_mode: true,
            secret: Some(secret.clone()),
            action: Some(dto.clone()),
        };

        set_json(
            &self.pool,
            MAINTENANCE_MODE_KEY,
            &serde_json::to_value(&state).unwrap_or_default(),
        )
        .await?;

        self.websocket.emit_app_restart(true);

        sign_maintenance_jwt(&secret, username)
    }

    async fn get_maintenance_mode(&self) -> Result<MaintenanceModeState, ErrorResp> {
        let value = get_json(&self.pool, MAINTENANCE_MODE_KEY).await?;
        Ok(value
            .and_then(|json| serde_json::from_value::<MaintenanceModeState>(json).ok())
            .unwrap_or(MaintenanceModeState {
                is_maintenance_mode: false,
                secret: None,
                action: None,
            }))
    }
}

fn generate_maintenance_secret() -> String {
    let mut bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn sign_maintenance_jwt(secret: &str, username: &str) -> Result<String, ErrorResp> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = MaintenanceClaims {
        username: username.to_string(),
        exp: now + 4 * 60 * 60,
        iat: now,
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|err| ErrorResp::ServerError(err.to_string()))
}

fn decode_maintenance_jwt(token: &str, secret: &str) -> Result<MaintenanceClaims, ErrorResp> {
    decode::<MaintenanceClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map(|data| data.claims)
    .map_err(|_| ErrorResp::Unauthorized("Invalid JWT Token".to_string()))
}

async fn check_marker_access(path: &Path) -> (bool, bool) {
    match tokio::fs::read(&path).await {
        Ok(_) => {
            let writable = tokio::fs::write(&path, format!("{}", chrono::Utc::now().timestamp()))
                .await
                .is_ok();
            (true, writable)
        }
        Err(_) => (false, false),
    }
}

async fn count_files(path: &Path) -> i32 {
    let mut stack = vec![path.to_path_buf()];
    let mut count = 0i32;

    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_name = entry.file_name();
            if file_name == ".immich" {
                continue;
            }

            let file_type = match entry.file_type().await {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };

            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                count += 1;
            }
        }
    }

    count
}
