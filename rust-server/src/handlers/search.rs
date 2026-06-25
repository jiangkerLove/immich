use axum::extract::{Query, State};
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::AssetResponse;
use crate::models::response::response::ErrorResp;
use crate::models::response::search::{
    PersonResponse, PlacesResponse, SearchExploreResponse, SearchResponse,
    SearchStatisticsResponse,
};
use crate::service::search::{
    LargeAssetSearchReq, MetadataSearchReq, RandomSearchReq, SearchPeopleQuery,
    SearchPlacesQuery, SearchSuggestionQuery, SmartSearchReq, StatisticsSearchReq,
};

pub async fn search_metadata_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<MetadataSearchReq>,
) -> Result<Json<SearchResponse>, ErrorResp> {
    Ok(Json(
        state.services.search.search_metadata(&auth, &dto).await?,
    ))
}

pub async fn search_statistics_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<StatisticsSearchReq>,
) -> Result<Json<SearchStatisticsResponse>, ErrorResp> {
    Ok(Json(
        state.services.search.search_statistics(&auth, &dto).await?,
    ))
}

pub async fn search_random_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<RandomSearchReq>,
) -> Result<Json<Vec<AssetResponse>>, ErrorResp> {
    Ok(Json(
        state.services.search.search_random(&auth, &dto).await?,
    ))
}

pub async fn search_large_assets_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(dto): Query<LargeAssetSearchReq>,
) -> Result<Json<Vec<AssetResponse>>, ErrorResp> {
    Ok(Json(
        state.services.search.search_large_assets(&auth, &dto).await?,
    ))
}

pub async fn search_smart_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<SmartSearchReq>,
) -> Result<Json<SearchResponse>, ErrorResp> {
    Ok(Json(
        state.services.search.search_smart(&auth, &dto).await?,
    ))
}

pub async fn get_explore_data_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<SearchExploreResponse>>, ErrorResp> {
    Ok(Json(
        state.services.search.get_explore_data(&auth).await?,
    ))
}

pub async fn search_person_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<SearchPeopleQuery>,
) -> Result<Json<Vec<PersonResponse>>, ErrorResp> {
    Ok(Json(
        state.services.search.search_person(&auth, &query).await?,
    ))
}

pub async fn search_places_handler(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthDto>,
    Query(query): Query<SearchPlacesQuery>,
) -> Result<Json<Vec<PlacesResponse>>, ErrorResp> {
    Ok(Json(state.services.search.search_places(&query).await?))
}

pub async fn get_assets_by_city_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<AssetResponse>>, ErrorResp> {
    Ok(Json(
        state.services.search.get_assets_by_city(&auth).await?,
    ))
}

pub async fn get_search_suggestions_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<SearchSuggestionQuery>,
) -> Result<Json<Vec<Option<String>>>, ErrorResp> {
    Ok(Json(
        state
            .services
            .search
            .get_search_suggestions(&auth, &query)
            .await?,
    ))
}
