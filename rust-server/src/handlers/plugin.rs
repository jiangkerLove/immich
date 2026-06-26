use axum::extract::{Path, Query, State};
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::plugin::{
    PluginMethodSearchQuery, PluginMethodResponse, PluginResponse, PluginSearchQuery,
    PluginTemplateResponse,
};

pub async fn search_plugins_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<PluginSearchQuery>,
) -> Result<Json<Vec<PluginResponse>>, ErrorResp> {
    Ok(Json(state.services.plugin.search(&auth, &query).await?))
}

pub async fn get_plugin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<PluginResponse>, ErrorResp> {
    Ok(Json(state.services.plugin.get(&auth, &id).await?))
}

pub async fn search_plugin_methods_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<PluginMethodSearchQuery>,
) -> Result<Json<Vec<PluginMethodResponse>>, ErrorResp> {
    Ok(Json(
        state.services.plugin.search_methods(&auth, &query).await?,
    ))
}

pub async fn search_plugin_templates_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<PluginTemplateResponse>>, ErrorResp> {
    Ok(Json(
        state.services.plugin.search_templates(&auth).await?,
    ))
}
