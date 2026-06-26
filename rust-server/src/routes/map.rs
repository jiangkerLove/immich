use axum::Router;
use axum::routing::get;

use crate::app_state::AppState;
use crate::handlers::map::{get_map_markers_handler, reverse_geocode_handler};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/map/markers", get(get_map_markers_handler))
        .route("/api/map/reverse-geocode", get(reverse_geocode_handler))
}
