use axum::Router;
use axum::routing::post;
use crate::app_state::AppState;
use crate::handlers::auth::login_handler;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/login", post(login_handler))
}