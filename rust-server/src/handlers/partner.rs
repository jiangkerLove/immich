use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::partner::{
    PartnerCreateReq, PartnerResponse, PartnerSearchQuery, PartnerUpdateReq,
};

pub async fn get_partners_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<PartnerSearchQuery>,
) -> Result<Json<Vec<PartnerResponse>>, ErrorResp> {
    Ok(Json(state.services.partner.search(&auth, &query).await?))
}

pub async fn create_partner_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<PartnerCreateReq>,
) -> Result<Json<PartnerResponse>, ErrorResp> {
    Ok(Json(state.services.partner.create(&auth, &dto).await?))
}

pub async fn create_partner_deprecated_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<PartnerResponse>, ErrorResp> {
    Ok(Json(
        state.services.partner.create_deprecated(&auth, &id).await?,
    ))
}

pub async fn update_partner_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<PartnerUpdateReq>,
) -> Result<Json<PartnerResponse>, ErrorResp> {
    Ok(Json(state.services.partner.update(&auth, &id, &dto).await?))
}

pub async fn delete_partner_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.partner.remove(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
