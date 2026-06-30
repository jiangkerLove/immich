use sqlx::PgPool;

use crate::models::db::notification::create_notification;
use crate::models::db::users::UserDb;
use crate::models::response::notification::{format_datetime, NotificationResponse};
use crate::service::websocket::WebSocketHub;

pub async fn on_job_error(
    pool: &PgPool,
    websocket: &WebSocketHub,
    job_name: &str,
    error: &str,
) {
    if job_name != "DatabaseBackup" {
        return;
    }

    let admin = match UserDb::get_admin(pool).await {
        Ok(Some(user)) => user,
        Ok(None) => return,
        Err(err) => {
            eprintln!("job error: failed to load admin user: {err}");
            return;
        }
    };

    eprintln!("Unable to run job handler ({job_name}): {error}");
    crate::utils::telemetry::record_job_status(job_name, "failed");

    let description = format!("Job {job_name} failed with error: {error}");
    let row = match create_notification(
        pool,
        &admin.id,
        "error",
        "JobFailed",
        "Job Failed",
        Some(&description),
        None,
        None,
    )
    .await
    {
        Ok(row) => row,
        Err(err) => {
            eprintln!("job error: failed to create admin notification: {err}");
            return;
        }
    };

    let response = NotificationResponse {
        id: row.id,
        created_at: format_datetime(&row.created_at),
        level: row.level,
        notification_type: row.notification_type,
        title: row.title,
        description: row.description,
        data: row.data,
        read_at: row.read_at.as_ref().map(format_datetime),
    };
    websocket.emit_notification(admin.id, response);
}
