use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::activity::{self, ActivityRow};
use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::access::require_album_access;
use crate::utils::permission::require_permission;

#[derive(Clone)]
pub struct ActivityService {
    pool: PgPool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityUserResponse {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub profile_image_path: String,
    pub avatar_color: String,
    pub profile_changed_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityResponse {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub user: ActivityUserResponse,
    pub asset_id: Option<Uuid>,
    #[serde(rename = "type")]
    pub activity_type: String,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityStatisticsResponse {
    pub comments: i64,
    pub likes: i64,
}

#[derive(Debug, Serialize)]
pub struct ActivityCreateResult {
    pub duplicate: bool,
    pub value: ActivityResponse,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySearchQuery {
    pub album_id: Uuid,
    pub asset_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub activity_type: Option<String>,
    pub level: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDto {
    pub album_id: Uuid,
    pub asset_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityCreateReq {
    pub album_id: Uuid,
    pub asset_id: Option<Uuid>,
    #[serde(rename = "type")]
    pub activity_type: String,
    pub comment: Option<String>,
}

impl ActivityService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_all(
        &self,
        auth: &AuthDto,
        query: &ActivitySearchQuery,
    ) -> Result<Vec<ActivityResponse>, ErrorResp> {
        require_album_access(&self.pool, auth, &query.album_id, Permission::AlbumRead).await?;

        let mut rows = activity::search_by_album(&self.pool, &query.album_id).await?;

        if let Some(user_id) = query.user_id {
            rows.retain(|row| row.user_id == user_id);
        }

        if query.level.as_deref() == Some("album") {
            rows.retain(|row| row.asset_id.is_none());
        } else if let Some(asset_id) = query.asset_id {
            rows.retain(|row| row.asset_id == Some(asset_id));
        }

        if let Some(activity_type) = &query.activity_type {
            match activity_type.as_str() {
                "like" => rows.retain(|row| row.is_liked),
                "comment" => rows.retain(|row| !row.is_liked),
                _ => {}
            }
        }

        Ok(rows.into_iter().map(map_activity).collect())
    }

    pub async fn get_statistics(
        &self,
        auth: &AuthDto,
        dto: &ActivityDto,
    ) -> Result<ActivityStatisticsResponse, ErrorResp> {
        require_permission(auth, Permission::ActivityStatistics)?;
        require_album_access(&self.pool, auth, &dto.album_id, Permission::AlbumRead).await?;

        let stats = activity::get_statistics(&self.pool, &dto.album_id, dto.asset_id.as_ref()).await?;
        Ok(ActivityStatisticsResponse {
            comments: stats.comments,
            likes: stats.likes,
        })
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &ActivityCreateReq,
    ) -> Result<ActivityCreateResult, ErrorResp> {
        require_permission(auth, Permission::ActivityCreate)?;

        if !activity::album_allows_activity(&self.pool, &auth.user.id, &dto.album_id).await? {
            return Err(ErrorResp::BadRequest(
                "Not found or no activity.create access".to_string(),
            ));
        }

        if let Some(asset_id) = dto.asset_id {
            if !activity::asset_in_album(&self.pool, &dto.album_id, &asset_id).await? {
                return Err(ErrorResp::BadRequest("Asset not in album".to_string()));
            }
        }

        let is_like = dto.activity_type == "like";
        if is_like {
            if dto.comment.is_some() {
                return Err(ErrorResp::BadRequest(
                    "Comment must not be provided for likes".to_string(),
                ));
            }
            if let Some(existing) =
                activity::find_like(&self.pool, &dto.album_id, &auth.user.id, dto.asset_id.as_ref())
                    .await?
            {
                return Ok(ActivityCreateResult {
                    duplicate: true,
                    value: map_activity(existing),
                });
            }
        } else if dto.activity_type == "comment" {
            let comment = dto
                .comment
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ErrorResp::BadRequest("Comment is required".to_string()))?;
            let row = activity::create(
                &self.pool,
                &dto.album_id,
                &auth.user.id,
                dto.asset_id.as_ref(),
                false,
                Some(comment),
            )
            .await?;
            return Ok(ActivityCreateResult {
                duplicate: false,
                value: map_activity(row),
            });
        } else {
            return Err(ErrorResp::BadRequest("Invalid activity type".to_string()));
        }

        let row = activity::create(
            &self.pool,
            &dto.album_id,
            &auth.user.id,
            dto.asset_id.as_ref(),
            true,
            None,
        )
        .await?;

        Ok(ActivityCreateResult {
            duplicate: false,
            value: map_activity(row),
        })
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::ActivityDelete)?;
        if !activity::can_delete(&self.pool, &auth.user.id, id).await? {
            return Err(ErrorResp::BadRequest(
                "Not found or no activity.delete access".to_string(),
            ));
        }
        activity::delete(&self.pool, id).await?;
        Ok(())
    }
}

fn map_activity(row: ActivityRow) -> ActivityResponse {
    ActivityResponse {
        id: row.id,
        created_at: row.created_at,
        user: map_activity_user(&row),
        asset_id: row.asset_id,
        activity_type: if row.is_liked {
            "like".to_string()
        } else {
            "comment".to_string()
        },
        comment: row.comment,
    }
}

fn map_activity_user(row: &ActivityRow) -> ActivityUserResponse {
    ActivityUserResponse {
        id: row.user_id,
        email: row.user_email.clone(),
        name: row.user_name.clone(),
        profile_image_path: row.user_profile_image_path.clone(),
        avatar_color: row
            .user_avatar_color
            .clone()
            .unwrap_or_else(|| email_to_avatar_color(&row.user_email)),
        profile_changed_at: row
            .user_profile_changed_at
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    }
}

fn email_to_avatar_color(email: &str) -> String {
    const COLORS: [&str; 10] = [
        "primary", "pink", "blue", "green", "yellow", "red", "purple", "orange", "gray", "amber",
    ];
    let sum: u32 = email.bytes().map(u32::from).sum();
    COLORS[(sum as usize) % COLORS.len()].to_string()
}
