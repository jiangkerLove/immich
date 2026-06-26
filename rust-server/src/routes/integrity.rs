use axum::Router;
use axum::routing::{delete, get};

use crate::app_state::AppState;
use crate::handlers::integrity::{
    delete_integrity_report_handler, get_integrity_report_csv_handler,
    get_integrity_report_file_handler, get_integrity_report_handler, get_integrity_summary_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/integrity/summary", get(get_integrity_summary_handler))
        .route("/api/admin/integrity/report", get(get_integrity_report_handler))
        .route(
            "/api/admin/integrity/report/{id}/file",
            get(get_integrity_report_file_handler),
        )
        .route(
            "/api/admin/integrity/report/{id}",
            delete(delete_integrity_report_handler),
        )
        .route(
            "/api/admin/integrity/report/{type}/csv",
            get(get_integrity_report_csv_handler),
        )
}
