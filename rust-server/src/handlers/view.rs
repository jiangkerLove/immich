use axum::extract::{Query, State};
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::AssetResponse;
use crate::models::response::response::ErrorResp;

#[derive(serde::Deserialize)]
pub struct FolderPathQuery {
    pub path: String,
}

pub async fn get_unique_original_paths_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<String>>, ErrorResp> {
    Ok(Json(
        state
            .services
            .view
            .get_unique_original_paths(&auth)
            .await?,
    ))
}

pub async fn get_assets_by_original_path_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<FolderPathQuery>,
) -> Result<Json<Vec<AssetResponse>>, ErrorResp> {
    Ok(Json(
        state
            .services
            .view
            .get_assets_by_original_path(&auth, &query.path)
            .await?,
    ))
}
