use axum::Router;
use axum::routing::{get, post};

use crate::app_state::AppState;
use crate::handlers::oauth::{
    authorize_handler, backchannel_logout_handler, callback_handler, link_handler,
    mobile_redirect_handler, unlink_handler,
};

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/api/oauth/mobile-redirect", get(mobile_redirect_handler))
        .route("/api/oauth/authorize", post(authorize_handler))
        .route("/api/oauth/callback", post(callback_handler))
        .route("/api/oauth/backchannel-logout", post(backchannel_logout_handler))
}

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/api/oauth/link", post(link_handler))
        .route("/api/oauth/unlink", post(unlink_handler))
}
