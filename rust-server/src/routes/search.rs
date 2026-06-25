use axum::Router;
use axum::routing::{get, post};

use crate::app_state::AppState;
use crate::handlers::search::{
    get_assets_by_city_handler, get_explore_data_handler, get_search_suggestions_handler,
    search_large_assets_handler, search_metadata_handler, search_person_handler,
    search_places_handler, search_random_handler, search_smart_handler,
    search_statistics_handler,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/search/metadata", post(search_metadata_handler))
        .route("/api/search/statistics", post(search_statistics_handler))
        .route("/api/search/random", post(search_random_handler))
        .route("/api/search/large-assets", post(search_large_assets_handler))
        .route("/api/search/smart", post(search_smart_handler))
        .route("/api/search/explore", get(get_explore_data_handler))
        .route("/api/search/person", get(search_person_handler))
        .route("/api/search/places", get(search_places_handler))
        .route("/api/search/cities", get(get_assets_by_city_handler))
        .route("/api/search/suggestions", get(get_search_suggestions_handler))
}
