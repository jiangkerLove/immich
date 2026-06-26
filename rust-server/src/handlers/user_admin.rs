use axum::extract::{Path, Query, State};
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::request::user::UserPreferencesUpdateReq;
use crate::models::response::asset::AssetStatsResponse;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::service::asset::AssetStatsQuery;
use crate::service::session::SessionResponse;
use crate::service::user_admin::{
    UserAdminCreateReq, UserAdminDeleteReq, UserAdminSearchQuery, UserAdminUpdateReq,
};
use crate::utils::calendar_heatmap::{CalendarHeatmapQuery, CalendarHeatmapResponse};

pub async fn search_users_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<UserAdminSearchQuery>,
) -> Result<Json<Vec<UserAdminResponse>>, ErrorResp> {
    Ok(Json(
        state.services.user_admin.search(&auth, &query).await?,
    ))
}

pub async fn create_user_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<UserAdminCreateReq>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.user_admin.create(&auth, &dto).await
}

pub async fn get_user_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.user_admin.get(&auth, &id).await
}

pub async fn update_user_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UserAdminUpdateReq>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.user_admin.update(&auth, &id, &dto).await
}

pub async fn patch_user_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UserAdminUpdateReq>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.user_admin.update(&auth, &id, &dto).await
}

pub async fn delete_user_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UserAdminDeleteReq>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.user_admin.delete(&auth, &id, &dto).await
}

pub async fn restore_user_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.user_admin.restore(&auth, &id).await
}

pub async fn get_user_calendar_heatmap_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Query(query): Query<CalendarHeatmapQuery>,
) -> Result<Json<CalendarHeatmapResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .user_admin
            .get_calendar_heatmap(&auth, &id, &query)
            .await?,
    ))
}

pub async fn get_user_sessions_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<SessionResponse>>, ErrorResp> {
    Ok(Json(
        state.services.user_admin.get_sessions(&auth, &id).await?,
    ))
}

pub async fn get_user_statistics_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Query(query): Query<AssetStatsQuery>,
) -> Result<Json<AssetStatsResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .user_admin
            .get_statistics(&auth, &id, &query)
            .await?,
    ))
}

pub async fn get_user_preferences_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResp> {
    Ok(Json(
        state.services.user_admin.get_preferences(&auth, &id).await?,
    ))
}

pub async fn update_user_preferences_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UserPreferencesUpdateReq>,
) -> Result<Json<serde_json::Value>, ErrorResp> {
    Ok(Json(
        state
            .services
            .user_admin
            .update_preferences(&auth, &id, &dto)
            .await?,
    ))
}

pub async fn patch_user_preferences_admin_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UserPreferencesUpdateReq>,
) -> Result<Json<serde_json::Value>, ErrorResp> {
    Ok(Json(
        state
            .services
            .user_admin
            .update_preferences(&auth, &id, &dto)
            .await?,
    ))
}
