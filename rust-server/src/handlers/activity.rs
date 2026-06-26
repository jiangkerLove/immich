use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::activity::{
    ActivityCreateReq, ActivityDto, ActivityResponse, ActivitySearchQuery,
    ActivityStatisticsResponse,
};

pub async fn get_activities_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<ActivitySearchQuery>,
) -> Result<Json<Vec<ActivityResponse>>, ErrorResp> {
    Ok(Json(state.services.activity.get_all(&auth, &query).await?))
}

pub async fn get_activity_statistics_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(dto): Query<ActivityDto>,
) -> Result<Json<ActivityStatisticsResponse>, ErrorResp> {
    Ok(Json(
        state.services.activity.get_statistics(&auth, &dto).await?,
    ))
}

pub async fn create_activity_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<ActivityCreateReq>,
) -> Result<(StatusCode, Json<ActivityResponse>), ErrorResp> {
    let result = state.services.activity.create(&auth, &dto).await?;
    let status = if result.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(result.value)))
}

pub async fn delete_activity_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.activity.delete(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
