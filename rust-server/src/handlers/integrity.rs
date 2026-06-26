use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{Response, StatusCode};
use axum::Extension;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::integrity::{
    IntegrityReportResponse, IntegrityReportSummaryResponse,
};

#[derive(Debug, Deserialize)]
pub struct IntegrityGetReportQuery {
    #[serde(rename = "type")]
    pub report_type: String,
    pub cursor: Option<Uuid>,
    pub limit: Option<i64>,
}

pub async fn get_integrity_summary_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<IntegrityReportSummaryResponse>, ErrorResp> {
    Ok(Json(
        state.services.integrity.get_summary(&auth).await?,
    ))
}

pub async fn get_integrity_report_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<IntegrityGetReportQuery>,
) -> Result<Json<IntegrityReportResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .integrity
            .get_report(&auth, &query.report_type, query.cursor, query.limit)
            .await?,
    ))
}

pub async fn get_integrity_report_file_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Response<Body>, ErrorResp> {
    state.services.integrity.get_report_file(&auth, &id).await
}

pub async fn delete_integrity_report_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.integrity.delete_report(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_integrity_report_csv_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(report_type): Path<String>,
) -> Result<Response<Body>, ErrorResp> {
    state
        .services
        .integrity
        .get_report_csv(&auth, &report_type)
        .await
}
