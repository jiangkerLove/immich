use axum::routing::get;
use axum::Router;

use crate::app_state::AppState;
use crate::handlers::system_metadata::{
    get_admin_onboarding_handler, update_admin_onboarding_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/system-metadata/admin-onboarding",
            get(get_admin_onboarding_handler).post(update_admin_onboarding_handler),
        )
}
