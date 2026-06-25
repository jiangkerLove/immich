use axum::Router;
use axum::routing::post;

use crate::app_state::AppState;
use crate::handlers::trash::{
    empty_trash_handler, restore_assets_handler, restore_trash_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/trash/empty", post(empty_trash_handler))
        .route("/api/trash/restore", post(restore_trash_handler))
        .route("/api/trash/restore/assets", post(restore_assets_handler))
}
