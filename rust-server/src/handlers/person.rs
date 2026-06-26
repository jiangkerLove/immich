use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::models::response::search::PersonResponse;
use crate::service::album::BulkIdResponse;
use crate::service::person::{
    AssetFaceUpdateReq, BulkIdsReq, MergePersonReq, PeopleResponse, PersonCreateReq,
    PersonSearchQuery, PersonStatisticsResponse, PersonUpdateReq, PeopleUpdateReq,
};

pub async fn get_people_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<PersonSearchQuery>,
) -> Result<Json<PeopleResponse>, ErrorResp> {
    Ok(Json(state.services.person.get_all(&auth, &query).await?))
}

pub async fn create_person_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<PersonCreateReq>,
) -> Result<Json<PersonResponse>, ErrorResp> {
    Ok(Json(state.services.person.create(&auth, &dto).await?))
}

pub async fn update_people_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<PeopleUpdateReq>,
) -> Result<Json<Vec<BulkIdResponse>>, ErrorResp> {
    Ok(Json(state.services.person.update_all(&auth, &dto).await?))
}

pub async fn delete_people_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<BulkIdsReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.person.delete_all(&auth, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_person_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<PersonResponse>, ErrorResp> {
    Ok(Json(state.services.person.get(&auth, &id).await?))
}

pub async fn update_person_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<PersonUpdateReq>,
) -> Result<Json<PersonResponse>, ErrorResp> {
    Ok(Json(state.services.person.update(&auth, &id, &dto).await?))
}

pub async fn delete_person_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.person.delete(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_person_statistics_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<PersonStatisticsResponse>, ErrorResp> {
    Ok(Json(
        state.services.person.get_statistics(&auth, &id).await?,
    ))
}

pub async fn get_person_thumbnail_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, ErrorResp> {
    state.services.person.get_thumbnail(&auth, &id).await
}

pub async fn reassign_faces_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<AssetFaceUpdateReq>,
) -> Result<Json<Vec<PersonResponse>>, ErrorResp> {
    Ok(Json(
        state.services.person.reassign_faces(&auth, &id, &dto).await?,
    ))
}

pub async fn merge_person_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<MergePersonReq>,
) -> Result<Json<Vec<BulkIdResponse>>, ErrorResp> {
    Ok(Json(state.services.person.merge(&auth, &id, &dto).await?))
}
