use axum::Router;
use axum::routing::get;

use crate::app_state::AppState;
use crate::handlers::notification::search_notifications_handler;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/notifications", get(search_notifications_handler))
}
