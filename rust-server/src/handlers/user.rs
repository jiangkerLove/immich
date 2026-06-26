use axum::extract::{Multipart, Path, Query, State};
use axum::Extension;
use axum::Json;
use axum::http::StatusCode;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::models::db::user_metadata::OnboardingPO;
use crate::models::dto::auth::AuthDto;
use crate::models::request::user::{UpdateUserMeReq, UserPreferencesUpdateReq};
use crate::models::response::response::ErrorResp;
use crate::models::response::user::{UserAdminResponse, UserResponse};
use crate::service::user::CreateProfileImageResponse;

pub async fn get_my_user_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.user.get_me(&auth).await
}

pub async fn get_my_preferences_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<serde_json::Value>, ErrorResp> {
    Ok(Json(
        state.services.user.get_me_preferences(&auth).await?,
    ))
}

pub async fn get_my_calendar_heatmap_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Query(query): Query<crate::utils::calendar_heatmap::CalendarHeatmapQuery>,
) -> Result<Json<crate::utils::calendar_heatmap::CalendarHeatmapResponse>, ErrorResp> {
    Ok(Json(
        state
            .services
            .user
            .get_calendar_heatmap(&auth, &query)
            .await?,
    ))
}

pub async fn update_my_user_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<UpdateUserMeReq>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.user.update_me(&auth, &dto).await
}

pub async fn patch_my_user_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<UpdateUserMeReq>,
) -> Result<UserAdminResponse, ErrorResp> {
    state.services.user.update_me(&auth, &dto).await
}

pub async fn update_my_preferences_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<UserPreferencesUpdateReq>,
) -> Result<Json<serde_json::Value>, ErrorResp> {
    Ok(Json(
        state.services.user.update_my_preferences(&auth, &dto).await?,
    ))
}

pub async fn patch_my_preferences_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<UserPreferencesUpdateReq>,
) -> Result<Json<serde_json::Value>, ErrorResp> {
    Ok(Json(
        state.services.user.update_my_preferences(&auth, &dto).await?,
    ))
}

pub async fn search_users_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<Vec<UserAdminResponse>>, ErrorResp> {
    Ok(Json(state.services.user.search(&auth).await?))
}

pub async fn get_my_onboarding_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<OnboardingPO>, ErrorResp> {
    Ok(Json(
        state.services.user.get_my_onboarding(&auth).await?,
    ))
}

pub async fn set_my_onboarding_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<OnboardingPO>,
) -> Result<Json<OnboardingPO>, ErrorResp> {
    Ok(Json(
        state.services.user.set_my_onboarding(&auth, &dto).await?,
    ))
}

pub async fn delete_my_onboarding_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<StatusCode, ErrorResp> {
    state.services.user.delete_my_onboarding(&auth).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_my_license_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<Json<crate::models::response::user::UserLicenseResponse>, ErrorResp> {
    Ok(Json(state.services.user.get_my_license(&auth).await?))
}

pub async fn set_my_license_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Json(dto): Json<crate::service::server::LicenseKeyReq>,
) -> Result<Json<crate::models::response::user::UserLicenseResponse>, ErrorResp> {
    Ok(Json(state.services.user.set_my_license(&auth, &dto).await?))
}

pub async fn delete_my_license_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<StatusCode, ErrorResp> {
    state.services.user.delete_my_license(&auth).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_profile_image_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    mut multipart: Multipart,
) -> Result<Json<CreateProfileImageResponse>, ErrorResp> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_name = String::from("profile.jpg");

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ErrorResp::BadRequest(err.to_string()))?
    {
        if field.name().unwrap_or("") == "file" {
            original_name = field
                .file_name()
                .unwrap_or("profile.jpg")
                .to_string();
            file_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|err| ErrorResp::BadRequest(err.to_string()))?
                    .to_vec(),
            );
        }
    }

    let file_bytes =
        file_bytes.ok_or_else(|| ErrorResp::BadRequest("file is required".to_string()))?;
    Ok(Json(
        state
            .services
            .user
            .create_profile_image(&auth, file_bytes, &original_name)
            .await?,
    ))
}

pub async fn delete_profile_image_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
) -> Result<StatusCode, ErrorResp> {
    state.services.user.delete_profile_image(&auth).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_user_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(id): Path<Uuid>,
) -> Result<Json<UserResponse>, ErrorResp> {
    Ok(Json(state.services.user.get(&auth, &id).await?))
}

pub async fn get_profile_image_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthDto>,
    Path(user_id): Path<Uuid>,
) -> Result<axum::response::Response, ErrorResp> {
    state
        .services
        .user
        .get_profile_image(&auth, &user_id)
        .await
}
