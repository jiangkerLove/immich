use axum::Router;
use axum::routing::{get, patch, post, put};

use crate::app_state::AppState;
use crate::handlers::user::{
    create_profile_image_handler, delete_my_license_handler, delete_my_onboarding_handler,
    delete_profile_image_handler, get_my_calendar_heatmap_handler, get_my_license_handler,
    get_my_onboarding_handler, get_my_preferences_handler, get_my_user_handler,
    get_profile_image_handler, get_user_handler, patch_my_preferences_handler, patch_my_user_handler,
    search_users_handler, set_my_license_handler, set_my_onboarding_handler,
    update_my_preferences_handler, update_my_user_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/users", get(search_users_handler))
        .route("/api/users/me", get(get_my_user_handler))
        .route("/api/users/me/preferences", get(get_my_preferences_handler))
        .route("/api/users/me", put(update_my_user_handler))
        .route("/api/users/me", patch(patch_my_user_handler))
        .route("/api/users/me/preferences", put(update_my_preferences_handler))
        .route("/api/users/me/preferences", patch(patch_my_preferences_handler))
        .route("/api/users/me/calendar-heatmap", get(get_my_calendar_heatmap_handler))
        .route("/api/users/me/license", get(get_my_license_handler))
        .route("/api/users/me/license", put(set_my_license_handler))
        .route("/api/users/me/license", axum::routing::delete(delete_my_license_handler))
        .route("/api/users/me/onboarding", get(get_my_onboarding_handler))
        .route("/api/users/me/onboarding", put(set_my_onboarding_handler))
        .route("/api/users/me/onboarding", axum::routing::delete(delete_my_onboarding_handler))
        .route("/api/users/profile-image", post(create_profile_image_handler))
        .route("/api/users/profile-image", axum::routing::delete(delete_profile_image_handler))
        .route("/api/users/{id}", get(get_user_handler))
        .route("/api/users/{id}/profile-image", get(get_profile_image_handler))
}
