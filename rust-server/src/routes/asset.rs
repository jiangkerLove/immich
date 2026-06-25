use axum::Router;
use axum::routing::{delete, get, put};

use crate::app_state::AppState;
use crate::handlers::asset::{
    delete_assets_handler, get_asset_handler, get_asset_statistics_handler, update_asset_handler,
    update_assets_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/assets/statistics", get(get_asset_statistics_handler))
        .route("/api/assets", put(update_assets_handler))
        .route("/api/assets", axum::routing::patch(update_assets_handler))
        .route("/api/assets", delete(delete_assets_handler))
        .route("/api/assets/{id}", get(get_asset_handler))
        .route("/api/assets/{id}", put(update_asset_handler))
        .route("/api/assets/{id}", axum::routing::patch(update_asset_handler))
}
