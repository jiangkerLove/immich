use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::face::AssetFaceResponse;
use crate::models::response::response::ErrorResp;
use crate::models::response::search::PersonResponse;
use crate::service::person::{
    AssetFaceCreateReq, AssetFaceDeleteReq, FaceQuery, FaceReassignReq,
};

pub async fn create_face_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<AssetFaceCreateReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.person.create_face(&auth, &dto).await?;
    Ok(StatusCode::CREATED)
}

pub async fn get_faces_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<FaceQuery>,
) -> Result<Json<Vec<AssetFaceResponse>>, ErrorResp> {
    Ok(Json(
        state
            .services
            .person
            .get_faces_by_asset(&auth, &query.id)
            .await?,
    ))
}

pub async fn reassign_face_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(person_id): Path<Uuid>,
    Json(dto): Json<FaceReassignReq>,
) -> Result<Json<PersonResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .person
            .reassign_face_by_id(&auth, &person_id, &dto.id)
            .await?,
    ))
}

pub async fn delete_face_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(face_id): Path<Uuid>,
    Json(dto): Json<AssetFaceDeleteReq>,
) -> Result<StatusCode, ErrorResp> {
    state.services.person.delete_face(&auth, &face_id, &dto).await?;
    Ok(StatusCode::NO_CONTENT)
}
