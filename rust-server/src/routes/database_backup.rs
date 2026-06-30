use axum::Router;
use axum::routing::{delete, get, post};

use crate::app_state::AppState;
use crate::handlers::database_backup::{
    delete_database_backups_handler, download_database_backup_handler,
    list_database_backups_handler, start_database_restore_handler, upload_database_backup_handler,
};

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/database-backups", get(list_database_backups_handler))
        .route(
            "/api/admin/database-backups/{filename}",
            get(download_database_backup_handler),
        )
        .route("/api/admin/database-backups", delete(delete_database_backups_handler))
        .route(
            "/api/admin/database-backups/upload",
            post(upload_database_backup_handler),
        )
}

pub fn public_router() -> Router<AppState> {
    Router::new().route(
        "/api/admin/database-backups/start-restore",
        post(start_database_restore_handler),
    )
}

pub fn maintenance_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/database-backups",
            get(crate::handlers::database_backup::maintenance_list_database_backups_handler),
        )
        .route(
            "/api/admin/database-backups/{filename}",
            get(crate::handlers::database_backup::maintenance_download_database_backup_handler),
        )
        .route(
            "/api/admin/database-backups",
            delete(crate::handlers::database_backup::maintenance_delete_database_backups_handler),
        )
        .route(
            "/api/admin/database-backups/upload",
            post(crate::handlers::database_backup::maintenance_upload_database_backup_handler),
        )
}
