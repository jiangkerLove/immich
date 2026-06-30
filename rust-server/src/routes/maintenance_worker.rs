use axum::Router;
use axum::routing::{get, post};

use crate::app_state::AppState;
use crate::handlers::maintenance::{
    maintenance_detect_prior_install_handler, maintenance_set_action_handler,
};
use crate::routes::{database_backup, maintenance, server, static_web};

pub fn router(web_root: Option<&std::path::Path>) -> Router<AppState> {
    let api = Router::new()
        .merge(server::public_router())
        .merge(maintenance::public_router())
        .route(
            "/api/admin/maintenance/detect-install",
            get(maintenance_detect_prior_install_handler),
        )
        .route("/api/admin/maintenance", post(maintenance_set_action_handler))
        .merge(database_backup::maintenance_router());

    if let Some(web_root) = web_root {
        api.merge(static_web::fallback_router(web_root))
    } else {
        api
    }
}
