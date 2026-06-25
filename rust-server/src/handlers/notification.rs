use axum::extract::{Query, State};
use axum::Extension;
use axum::Json;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::notification::NotificationResponse;
use crate::models::response::response::ErrorResp;
use crate::service::notification::NotificationSearchQuery;

pub async fn search_notifications_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<NotificationSearchQuery>,
) -> Result<Json<Vec<NotificationResponse>>, ErrorResp> {
    Ok(Json(
        state.services.notification.search(&auth, &query).await?,
    ))
}
