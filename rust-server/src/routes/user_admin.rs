use axum::Router;
use axum::routing::{delete, get, patch, post, put};

use crate::app_state::AppState;
use crate::handlers::user_admin::{
    create_user_admin_handler, delete_user_admin_handler, get_user_admin_handler,
    get_user_calendar_heatmap_admin_handler, get_user_preferences_admin_handler,
    get_user_sessions_admin_handler, get_user_statistics_admin_handler,
    patch_user_admin_handler, patch_user_preferences_admin_handler, restore_user_admin_handler,
    search_users_admin_handler, update_user_admin_handler, update_user_preferences_admin_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/users", get(search_users_admin_handler))
        .route("/api/admin/users", post(create_user_admin_handler))
        .route("/api/admin/users/{id}", get(get_user_admin_handler))
        .route("/api/admin/users/{id}", put(update_user_admin_handler))
        .route("/api/admin/users/{id}", patch(patch_user_admin_handler))
        .route("/api/admin/users/{id}", delete(delete_user_admin_handler))
        .route(
            "/api/admin/users/{id}/calendar-heatmap",
            get(get_user_calendar_heatmap_admin_handler),
        )
        .route(
            "/api/admin/users/{id}/sessions",
            get(get_user_sessions_admin_handler),
        )
        .route(
            "/api/admin/users/{id}/statistics",
            get(get_user_statistics_admin_handler),
        )
        .route(
            "/api/admin/users/{id}/preferences",
            get(get_user_preferences_admin_handler),
        )
        .route(
            "/api/admin/users/{id}/preferences",
            put(update_user_preferences_admin_handler),
        )
        .route(
            "/api/admin/users/{id}/preferences",
            patch(patch_user_preferences_admin_handler),
        )
        .route(
            "/api/admin/users/{id}/restore",
            post(restore_user_admin_handler),
        )
}
