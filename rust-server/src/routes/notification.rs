use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::notification::{
    admin_create_notification_handler, admin_render_template_handler, admin_send_test_email_handler,
    delete_notification_handler, delete_notifications_handler, get_notification_handler,
    search_notifications_handler, update_notification_handler, update_notifications_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/notifications", get(search_notifications_handler))
        .route("/api/notifications", put(update_notifications_handler))
        .route("/api/notifications", delete(delete_notifications_handler))
        .route("/api/notifications/{id}", get(get_notification_handler))
        .route("/api/notifications/{id}", put(update_notification_handler))
        .route("/api/notifications/{id}", delete(delete_notification_handler))
        .route(
            "/api/admin/notifications",
            post(admin_create_notification_handler),
        )
        .route(
            "/api/admin/notifications/test-email",
            post(admin_send_test_email_handler),
        )
        .route(
            "/api/admin/notifications/templates/{name}",
            post(admin_render_template_handler),
        )
}
