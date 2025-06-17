use axum::Router;
use axum::routing::get;
use crate::app_state::AppState;
use crate::handlers::user::{get_my_preferences_handler, get_my_user_handler};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/users/me", get(get_my_user_handler))
        .route("/api/users/me/preferences", get(get_my_preferences_handler))
}