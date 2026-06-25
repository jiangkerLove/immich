use axum::Router;
use axum::routing::{get, post, put};

use crate::app_state::AppState;
use crate::handlers::user::{
    get_my_onboarding_handler, get_my_preferences_handler, get_my_user_handler,
    search_users_handler, set_my_onboarding_handler,
};
use crate::handlers::stub;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/users", get(search_users_handler))
        .route("/api/users/me", get(get_my_user_handler))
        .route("/api/users/me/preferences", get(get_my_preferences_handler))
        .route("/api/users/me", put(stub::not_implemented))
        .route("/api/users/me", axum::routing::patch(stub::not_implemented))
        .route("/api/users/me/preferences", put(stub::not_implemented))
        .route("/api/users/me/preferences", axum::routing::patch(stub::not_implemented))
        .route("/api/users/me/calendar-heatmap", get(stub::not_implemented))
        .route("/api/users/me/license", get(stub::not_implemented))
        .route("/api/users/me/license", put(stub::not_implemented))
        .route("/api/users/me/license", axum::routing::delete(stub::not_implemented))
        .route("/api/users/me/onboarding", get(get_my_onboarding_handler))
        .route("/api/users/me/onboarding", put(set_my_onboarding_handler))
        .route("/api/users/me/onboarding", axum::routing::delete(stub::not_implemented))
        .route("/api/users/profile-image", post(stub::not_implemented))
        .route("/api/users/profile-image", axum::routing::delete(stub::not_implemented))
        .route("/api/users/{id}", get(stub::not_implemented))
        .route("/api/users/{id}/profile-image", get(stub::not_implemented))
}
