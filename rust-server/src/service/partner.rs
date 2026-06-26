use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::album;
use crate::models::db::partner::{self, PartnerDirection, PartnerRow};
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::utils::permission::require_permission;
use crate::models::db::auth_permission::Permission;

#[derive(Clone)]
pub struct PartnerService {
    pool: PgPool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartnerResponse {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub profile_image_path: String,
    pub avatar_color: String,
    pub profile_changed_at: String,
    pub in_timeline: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartnerCreateReq {
    pub shared_with_id: Uuid,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartnerUpdateReq {
    pub in_timeline: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartnerSearchQuery {
    pub direction: String,
}

impl PartnerService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn search(
        &self,
        auth: &AuthDto,
        query: &PartnerSearchQuery,
    ) -> Result<Vec<PartnerResponse>, ErrorResp> {
        require_permission(auth, Permission::PartnerRead)?;
        let direction = PartnerDirection::parse(&query.direction).ok_or_else(|| {
            ErrorResp::BadRequest("Invalid partner direction".to_string())
        })?;

        let rows = partner::get_all(&self.pool, &auth.user.id).await?;
        Ok(rows
            .into_iter()
            .filter(|row| match direction {
                PartnerDirection::SharedBy => row.shared_by_id == auth.user.id,
                PartnerDirection::SharedWith => row.shared_with_id == auth.user.id,
            })
            .map(map_partner)
            .collect())
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &PartnerCreateReq,
    ) -> Result<PartnerResponse, ErrorResp> {
        require_permission(auth, Permission::PartnerCreate)?;

        if dto.shared_with_id == auth.user.id {
            return Err(ErrorResp::BadRequest("Cannot share with yourself".to_string()));
        }

        if !album::user_exists(&self.pool, &dto.shared_with_id).await? {
            return Err(ErrorResp::BadRequest("User not found".to_string()));
        }

        if partner::get(&self.pool, &auth.user.id, &dto.shared_with_id)
            .await?
            .is_some()
        {
            return Err(ErrorResp::BadRequest("Partner already exists".to_string()));
        }

        let row = partner::create(&self.pool, &auth.user.id, &dto.shared_with_id).await?;
        Ok(map_partner(row))
    }

    pub async fn create_deprecated(
        &self,
        auth: &AuthDto,
        shared_with_id: &Uuid,
    ) -> Result<PartnerResponse, ErrorResp> {
        self.create(auth, &PartnerCreateReq { shared_with_id: *shared_with_id })
            .await
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        shared_by_id: &Uuid,
        dto: &PartnerUpdateReq,
    ) -> Result<PartnerResponse, ErrorResp> {
        require_permission(auth, Permission::PartnerUpdate)?;

        if !partner::partner_exists_for_update(&self.pool, shared_by_id, &auth.user.id).await? {
            return Err(ErrorResp::BadRequest(
                "Not found or no partner.update access".to_string(),
            ));
        }

        let row = partner::update_in_timeline(
            &self.pool,
            shared_by_id,
            &auth.user.id,
            dto.in_timeline,
        )
        .await?;
        Ok(map_partner(row))
    }

    pub async fn remove(&self, auth: &AuthDto, shared_with_id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::PartnerDelete)?;

        if partner::get(&self.pool, &auth.user.id, shared_with_id)
            .await?
            .is_none()
        {
            return Err(ErrorResp::BadRequest("Partner not found".to_string()));
        }

        partner::remove(&self.pool, &auth.user.id, shared_with_id).await?;
        Ok(())
    }
}

fn map_partner(row: PartnerRow) -> PartnerResponse {
    let avatar_color = row
        .avatar_color
        .clone()
        .unwrap_or_else(|| email_to_avatar_color(&row.email));
    PartnerResponse {
        id: row.user_id,
        email: row.email,
        name: row.name,
        profile_image_path: row.profile_image_path,
        avatar_color,
        profile_changed_at: format_datetime(&row.profile_changed_at),
        in_timeline: row.in_timeline,
    }
}

fn format_datetime(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn email_to_avatar_color(email: &str) -> String {
    const COLORS: [&str; 10] = [
        "primary", "pink", "blue", "green", "yellow", "red", "purple", "orange", "gray", "amber",
    ];
    let sum: u32 = email.bytes().map(u32::from).sum();
    COLORS[(sum as usize) % COLORS.len()].to_string()
}
