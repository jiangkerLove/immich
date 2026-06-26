use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceAction {
    Start,
    End,
    SelectDatabaseRestore,
    RestoreDatabase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetMaintenanceModeReq {
    pub action: MaintenanceAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_backup_filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceLoginReq {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceAuthResp {
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceStatusResp {
    pub active: bool,
    pub action: MaintenanceAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceDetectInstallFolderResp {
    pub folder: String,
    pub readable: bool,
    pub writable: bool,
    pub files: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceDetectInstallResp {
    pub storage: Vec<MaintenanceDetectInstallFolderResp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceModeState {
    pub is_maintenance_mode: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<SetMaintenanceModeReq>,
}
