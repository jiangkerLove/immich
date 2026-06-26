use axum::Router;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::handlers::asset::{
    copy_asset_handler, delete_asset_edits_handler, delete_asset_metadata_by_key_handler,
    delete_assets_handler, delete_bulk_asset_metadata_handler, get_asset_edits_handler,
    get_asset_handler, get_asset_metadata_by_key_handler, get_asset_metadata_handler,
    get_asset_ocr_handler, get_asset_statistics_handler, replace_asset_edits_handler, run_asset_jobs_handler,
    update_asset_handler, update_assets_handler, upsert_asset_metadata_handler,
    upsert_bulk_asset_metadata_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/assets/copy", put(copy_asset_handler))
        .route("/api/assets/jobs", post(run_asset_jobs_handler))
        .route("/api/assets/statistics", get(get_asset_statistics_handler))
        .route("/api/assets/metadata", put(upsert_bulk_asset_metadata_handler))
        .route("/api/assets/metadata", delete(delete_bulk_asset_metadata_handler))
        .route("/api/assets", put(update_assets_handler))
        .route("/api/assets", axum::routing::patch(update_assets_handler))
        .route("/api/assets", delete(delete_assets_handler))
        .route("/api/assets/{id}", get(get_asset_handler))
        .route("/api/assets/{id}", put(update_asset_handler))
        .route("/api/assets/{id}", axum::routing::patch(update_asset_handler))
        .route("/api/assets/{id}/metadata", get(get_asset_metadata_handler))
        .route("/api/assets/{id}/metadata", put(upsert_asset_metadata_handler))
        .route(
            "/api/assets/{id}/metadata/{key}",
            get(get_asset_metadata_by_key_handler),
        )
        .route(
            "/api/assets/{id}/metadata/{key}",
            delete(delete_asset_metadata_by_key_handler),
        )
        .route("/api/assets/{id}/edits", get(get_asset_edits_handler))
        .route("/api/assets/{id}/edits", put(replace_asset_edits_handler))
        .route("/api/assets/{id}/edits", delete(delete_asset_edits_handler))
        .route("/api/assets/{id}/ocr", get(get_asset_ocr_handler))
}
