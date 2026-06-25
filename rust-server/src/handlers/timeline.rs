use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, Response};
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::timeline::{TimeBucketAssetQuery, TimeBucketQuery};
use crate::models::db::timeline::TimeBucketItem;

pub async fn get_time_buckets_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<TimeBucketQuery>,
) -> Result<Json<Vec<TimeBucketItem>>, ErrorResp> {
    Ok(Json(
        state.services.timeline.get_time_buckets(&auth, &query).await?,
    ))
}

pub async fn get_time_bucket_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<TimeBucketAssetQuery>,
) -> Result<Response<Body>, ErrorResp> {
    let body = state.services.timeline.get_time_bucket(&auth, &query).await?;
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .map_err(|e| ErrorResp::ServerError(e.to_string()))
}
