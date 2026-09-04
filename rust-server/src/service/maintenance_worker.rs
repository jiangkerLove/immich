use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::RwLock;

use crate::models::db::advisory_lock::{self, LOCK_MAINTENANCE_OPERATION};
use crate::models::db::system_metadata::{get_json, set_json};
use crate::models::dto::env::EnvDto;
use crate::models::dto::maintenance::{
    MaintenanceAction, MaintenanceAuthResp, MaintenanceDetectInstallResp, MaintenanceLoginReq,
    MaintenanceModeState, MaintenanceStatusResp, SetMaintenanceModeReq,
};
use crate::models::response::response::ErrorResp;
use crate::service::database_backup_runner::DatabaseBackupRunner;
use crate::service::maintenance::{
    MAINTENANCE_MODE_KEY, MaintenanceClaims, decode_maintenance_jwt, detect_prior_install_internal,
    generate_maintenance_secret, public_maintenance_status, sign_maintenance_jwt,
};
use crate::service::websocket::WebSocketHub;
use crate::utils::storage::StoragePaths;

#[derive(Clone)]
pub struct MaintenanceWorkerRuntime {
    pool: PgPool,
    storage: StoragePaths,
    env: EnvDto,
    websocket: WebSocketHub,
    secret: Arc<RwLock<String>>,
    status: Arc<RwLock<MaintenanceStatusResp>>,
}

impl MaintenanceWorkerRuntime {
    pub async fn spawn(
        pool: PgPool,
        storage: StoragePaths,
        env: EnvDto,
        websocket: WebSocketHub,
    ) -> Self {
        let runtime = Self {
            pool: pool.clone(),
            storage: storage.clone(),
            env: env.clone(),
            websocket: websocket.clone(),
            secret: Arc::new(RwLock::new(String::new())),
            status: Arc::new(RwLock::new(MaintenanceStatusResp {
                active: true,
                action: MaintenanceAction::Start,
                progress: None,
                task: None,
                error: None,
            })),
        };

        runtime.init().await;
        runtime
    }

    async fn init(&self) {
        let state = match get_json(&self.pool, MAINTENANCE_MODE_KEY).await {
            Ok(value) => value
                .and_then(|json| serde_json::from_value::<MaintenanceModeState>(json).ok())
                .filter(|state| state.is_maintenance_mode),
            Err(err) => {
                eprintln!("maintenance worker init failed: {err}");
                return;
            }
        };

        let Some(state) = state else {
            eprintln!("maintenance worker started without maintenance-mode metadata");
            return;
        };

        let secret = state
            .secret
            .clone()
            .unwrap_or_else(generate_maintenance_secret);
        *self.secret.write().await = secret.clone();

        let action = state
            .action
            .as_ref()
            .map(|a| a.action)
            .unwrap_or(MaintenanceAction::Start);
        self.set_status(MaintenanceStatusResp {
            active: true,
            action,
            progress: None,
            task: None,
            error: None,
        })
        .await;

        self.log_secret(&secret).await;

        if let Some(action) = state.action {
            self.run_action(action).await;
        }
    }

    pub async fn status(&self, jwt: Option<&str>) -> MaintenanceStatusResp {
        if let Some(token) = jwt {
            if self.login(token).await.is_ok() {
                return self.status.read().await.clone();
            }
        }
        public_maintenance_status(&*self.status.read().await)
    }

    pub async fn maintenance_login(
        &self,
        dto: &MaintenanceLoginReq,
    ) -> Result<MaintenanceAuthResp, ErrorResp> {
        let token = dto
            .token
            .as_deref()
            .ok_or_else(|| ErrorResp::Unauthorized("Missing JWT Token".to_string()))?;
        let claims = self.login(token).await?;
        Ok(MaintenanceAuthResp {
            username: claims.username,
        })
    }

    pub async fn detect_prior_install(&self) -> Result<MaintenanceDetectInstallResp, ErrorResp> {
        detect_prior_install_internal(&self.storage).await
    }

    pub async fn set_action(&self, dto: SetMaintenanceModeReq) {
        self.set_status(MaintenanceStatusResp {
            active: true,
            action: dto.action,
            progress: None,
            task: None,
            error: None,
        })
        .await;
        self.run_action(dto).await;
    }

    async fn run_action(&self, dto: SetMaintenanceModeReq) {
        match dto.action {
            MaintenanceAction::Start | MaintenanceAction::SelectDatabaseRestore => {}
            MaintenanceAction::End => {
                self.end_maintenance().await;
            }
            MaintenanceAction::RestoreDatabase => {
                self.run_restore_database(dto).await;
            }
        }
    }

    async fn run_restore_database(&self, dto: SetMaintenanceModeReq) {
        let Some(_lock) = advisory_lock::try_acquire(&self.pool, LOCK_MAINTENANCE_OPERATION)
            .await
            .ok()
            .flatten()
        else {
            return;
        };

        let secret = self.secret.read().await.clone();
        let reset_state = MaintenanceModeState {
            is_maintenance_mode: true,
            secret: Some(secret),
            action: Some(SetMaintenanceModeReq {
                action: MaintenanceAction::Start,
                restore_backup_filename: None,
            }),
        };
        let _ = set_json(
            &self.pool,
            MAINTENANCE_MODE_KEY,
            &serde_json::to_value(&reset_state).unwrap_or_default(),
        )
        .await;

        let filename = match dto.restore_backup_filename {
            Some(name) if !name.is_empty() => name,
            _ => {
                self.set_status(MaintenanceStatusResp {
                    active: true,
                    action: MaintenanceAction::RestoreDatabase,
                    progress: None,
                    task: Some("error".into()),
                    error: Some("Expected restoreBackupFilename but it's missing!".into()),
                })
                .await;
                return;
            }
        };

        let runner =
            DatabaseBackupRunner::new(self.pool.clone(), self.storage.clone(), self.env.clone());
        let runtime = self.clone();
        let result = runner
            .restore_database_backup(&filename, move |task, progress| {
                let rt = runtime.clone();
                let task = task.to_string();
                tokio::spawn(async move {
                    rt.set_status(MaintenanceStatusResp {
                        active: true,
                        action: MaintenanceAction::RestoreDatabase,
                        progress: Some(progress),
                        task: Some(task),
                        error: None,
                    })
                    .await;
                });
            })
            .await;

        if let Err(err) = result {
            eprintln!("maintenance restore failed: {err}");
            self.set_status(MaintenanceStatusResp {
                active: true,
                action: MaintenanceAction::RestoreDatabase,
                progress: None,
                task: Some("error".into()),
                error: Some(err.to_string()),
            })
            .await;
            return;
        }

        self.end_maintenance().await;
    }

    async fn end_maintenance(&self) {
        let state = MaintenanceModeState {
            is_maintenance_mode: false,
            secret: None,
            action: None,
        };
        let _ = set_json(
            &self.pool,
            MAINTENANCE_MODE_KEY,
            &serde_json::to_value(&state).unwrap_or_default(),
        )
        .await;

        self.websocket.emit_maintenance_end();
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            std::process::exit(0);
        });
    }

    async fn set_status(&self, status: MaintenanceStatusResp) {
        *self.status.write().await = status.clone();
        self.websocket.emit_maintenance_status(&status);
    }

    async fn login(&self, token: &str) -> Result<MaintenanceClaims, ErrorResp> {
        let secret = self.secret.read().await.clone();
        decode_maintenance_jwt(token, &secret)
    }

    async fn log_secret(&self, secret: &str) {
        let host = self.env.immich_host.as_deref().unwrap_or("localhost");
        let port = self.env.immich_port.unwrap_or(2283);
        if let Ok(jwt) = sign_maintenance_jwt(secret, "immich-admin") {
            println!(
                "\n\n🚧 Immich is in maintenance mode, you can log in using the following URL:\nhttp://{host}:{port}/maintenance?token={jwt}\n"
            );
        }
    }
}
