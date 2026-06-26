use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::dto::auth::AuthDto;
use crate::models::response::notification::NotificationResponse;
use crate::models::response::response::ErrorResp;
use crate::service::notification::{
    NotificationCreateReq, NotificationDeleteAllReq, NotificationSearchQuery, NotificationUpdateAllReq,
    NotificationUpdateReq,
};

pub async fn search_notifications_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<NotificationSearchQuery>,
) -> Result<Json<Vec<NotificationResponse>>, ErrorResp> {
    Ok(Json(
        state.services.notification.search(&auth, &query).await?,
    ))
}

pub async fn update_notifications_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<NotificationUpdateAllReq>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .notification
        .update_all(&auth, &dto)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_notifications_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<NotificationDeleteAllReq>,
) -> Result<StatusCode, ErrorResp> {
    state
        .services
        .notification
        .delete_all(&auth, &dto)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_notification_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<NotificationResponse>, ErrorResp> {
    Ok(Json(state.services.notification.get(&auth, &id).await?))
}

pub async fn update_notification_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
    Json(dto): Json<NotificationUpdateReq>,
) -> Result<Json<NotificationResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .notification
            .update(&auth, &id, &dto)
            .await?,
    ))
}

pub async fn delete_notification_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ErrorResp> {
    state.services.notification.delete(&auth, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn admin_create_notification_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<NotificationCreateReq>,
) -> Result<Json<NotificationResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .notification
            .admin_create(&auth, &dto)
            .await?,
    ))
}

pub async fn admin_send_test_email_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<crate::service::email::SmtpConfig>,
) -> Result<Json<crate::service::notification::TestEmailResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .notification
            .admin_send_test_email(&auth, &dto)
            .await?,
    ))
}

pub async fn admin_render_template_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(name): Path<String>,
    Json(dto): Json<crate::service::notification::TemplatePreviewReq>,
) -> Result<Json<crate::service::notification::TemplatePreviewResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .notification
            .admin_render_template(&auth, &name, &dto)
            .await?,
    ))
}
