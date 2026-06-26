use axum::extract::{Query, State};
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::map::{
    MapMarkerQuery, MapMarkerResponse, MapReverseGeocodeQuery, MapReverseGeocodeResponse,
};
use crate::utils::permission::require_permission;

pub async fn get_map_markers_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<MapMarkerQuery>,
) -> Result<Json<Vec<MapMarkerResponse>>, ErrorResp> {
    Ok(Json(
        state.services.map.get_map_markers(&auth, &query).await?,
    ))
}

pub async fn reverse_geocode_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<MapReverseGeocodeQuery>,
) -> Result<Json<Vec<MapReverseGeocodeResponse>>, ErrorResp> {
    require_permission(&auth, Permission::MapSearch)?;
    Ok(Json(state.services.map.reverse_geocode(&query).await?))
}
