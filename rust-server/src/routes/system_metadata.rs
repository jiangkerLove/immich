use axum::routing::get;
use axum::Router;

use crate::app_state::AppState;
use crate::handlers::system_metadata::{
    get_admin_onboarding_handler, get_reverse_geocoding_state_handler,
    get_version_check_state_handler, update_admin_onboarding_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/system-metadata/admin-onboarding",
            get(get_admin_onboarding_handler).post(update_admin_onboarding_handler),
        )
        .route(
            "/api/system-metadata/reverse-geocoding-state",
            get(get_reverse_geocoding_state_handler),
        )
        .route(
            "/api/system-metadata/version-check-state",
            get(get_version_check_state_handler),
        )
}
