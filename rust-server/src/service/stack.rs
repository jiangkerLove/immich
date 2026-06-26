use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::assets;
use crate::models::db::stack;
use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::{map_assets, AssetResponse};
use crate::models::response::response::ErrorResp;
use crate::service::access::require_assets_access;
use crate::service::websocket::WebSocketHub;
use crate::utils::permission::require_permission;
use crate::models::db::auth_permission::Permission;

#[derive(Clone)]
pub struct StackService {
    pool: PgPool,
    websocket: WebSocketHub,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackResponse {
    pub id: Uuid,
    pub primary_asset_id: Uuid,
    pub assets: Vec<AssetResponse>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StackSearchQuery {
    pub primary_asset_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackCreateReq {
    pub asset_ids: Vec<Uuid>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StackUpdateReq {
    pub primary_asset_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkIdsReq {
    pub ids: Vec<Uuid>,
}

impl StackService {
    pub fn new(pool: PgPool, websocket: WebSocketHub) -> Self {
        Self { pool, websocket }
    }

    pub async fn search(
        &self,
        auth: &AuthDto,
        query: &StackSearchQuery,
    ) -> Result<Vec<StackResponse>, ErrorResp> {
        require_permission(auth, Permission::StackRead)?;
        let rows = stack::search(
            &self.pool,
            &auth.user.id,
            query.primary_asset_id.as_ref(),
        )
        .await?;

        let mut responses = Vec::with_capacity(rows.len());
        for row in rows {
            responses.push(self.map_stack(auth, &row.id, &row.primary_asset_id).await?);
        }
        Ok(responses)
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &StackCreateReq,
    ) -> Result<StackResponse, ErrorResp> {
        require_permission(auth, Permission::StackCreate)?;

        if dto.asset_ids.len() < 2 {
            return Err(ErrorResp::BadRequest(
                "At least 2 assets are required".to_string(),
            ));
        }

        require_assets_access(&self.pool, auth, &dto.asset_ids, Permission::AssetUpdate).await?;

        let row = stack::create(&self.pool, &auth.user.id, &dto.asset_ids).await?;
        self.websocket.emit_stack_update(auth.user.id);
        self.map_stack(auth, &row.id, &row.primary_asset_id).await
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<StackResponse, ErrorResp> {
        require_permission(auth, Permission::StackRead)?;
        self.require_stack_owner(auth, &[*id]).await?;
        let row = stack::get_by_id(&self.pool, id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Asset stack not found".to_string()))?;
        self.map_stack(auth, &row.id, &row.primary_asset_id).await
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &StackUpdateReq,
    ) -> Result<StackResponse, ErrorResp> {
        require_permission(auth, Permission::StackUpdate)?;
        self.require_stack_owner(auth, &[*id]).await?;

        let Some(primary_asset_id) = dto.primary_asset_id else {
            return Err(ErrorResp::BadRequest("No fields to update".to_string()));
        };

        let asset_ids = stack::list_asset_ids(&self.pool, id).await?;
        if !asset_ids.contains(&primary_asset_id) {
            return Err(ErrorResp::BadRequest(
                "Primary asset must be in the stack".to_string(),
            ));
        }

        let row = stack::update_primary(&self.pool, id, &primary_asset_id).await?;
        self.websocket.emit_stack_update(auth.user.id);
        self.map_stack(auth, &row.id, &row.primary_asset_id).await
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::StackDelete)?;
        self.require_stack_owner(auth, &[*id]).await?;
        stack::delete(&self.pool, id).await?;
        self.websocket.emit_stack_update(auth.user.id);
        Ok(())
    }

    pub async fn delete_all(&self, auth: &AuthDto, dto: &BulkIdsReq) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::StackDelete)?;
        self.require_stack_owner(auth, &dto.ids).await?;
        stack::delete_all(&self.pool, &dto.ids).await?;
        self.websocket.emit_stack_update(auth.user.id);
        Ok(())
    }

    pub async fn remove_asset(
        &self,
        auth: &AuthDto,
        stack_id: &Uuid,
        asset_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::StackUpdate)?;
        self.require_stack_owner(auth, &[*stack_id]).await?;

        let row = stack::get_for_asset_removal(&self.pool, asset_id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Asset not in stack".to_string()))?;

        if row.id != Some(*stack_id) {
            return Err(ErrorResp::BadRequest("Asset not in stack".to_string()));
        }

        if row.primary_asset_id == Some(*asset_id) {
            return Err(ErrorResp::BadRequest(
                "Cannot remove stack's primary asset".to_string(),
            ));
        }

        stack::remove_asset_from_stack(&self.pool, asset_id).await?;
        self.websocket.emit_stack_update(auth.user.id);
        Ok(())
    }

    async fn require_stack_owner(&self, auth: &AuthDto, ids: &[Uuid]) -> Result<(), ErrorResp> {
        if !stack::owner_owns_stacks(&self.pool, &auth.user.id, ids).await? {
            return Err(ErrorResp::BadRequest(format!(
                "Not found or no stack access"
            )));
        }
        Ok(())
    }

    async fn map_stack(
        &self,
        auth: &AuthDto,
        stack_id: &Uuid,
        primary_asset_id: &Uuid,
    ) -> Result<StackResponse, ErrorResp> {
        let asset_ids = stack::list_asset_ids(&self.pool, stack_id).await?;
        let rows = assets::get_details_by_ids(&self.pool, &asset_ids).await?;
        let mut mapped = map_assets(&self.pool, &rows, auth, false).await?;

        mapped.sort_by_key(|asset| asset.id != *primary_asset_id);
        if !mapped.iter().any(|asset| asset.id == *primary_asset_id) {
            if let Some(row) = assets::get_detail_by_id(&self.pool, primary_asset_id).await? {
                let mut single = map_assets(&self.pool, std::slice::from_ref(&row), auth, false).await?;
                mapped.splice(0..0, single.drain(..));
            }
        }

        Ok(StackResponse {
            id: *stack_id,
            primary_asset_id: *primary_asset_id,
            assets: mapped,
        })
    }
}
