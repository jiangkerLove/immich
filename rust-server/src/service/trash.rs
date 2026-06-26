use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::auth_permission::Permission;
use crate::models::db::trash;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::access::require_assets_access;
use crate::service::job::JobService;
use crate::service::websocket::WebSocketHub;

#[derive(Clone)]
pub struct TrashService {
    pool: PgPool,
    jobs: JobService,
    websocket: WebSocketHub,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkIdsReq {
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashResponse {
    pub count: u64,
}

impl TrashService {
    pub fn new(pool: PgPool, jobs: JobService, websocket: WebSocketHub) -> Self {
        Self {
            pool,
            jobs,
            websocket,
        }
    }

    pub async fn empty(&self, auth: &AuthDto) -> Result<TrashResponse, ErrorResp> {
        let count = trash::empty_for_user(&self.pool, &auth.user.id).await?;
        if count > 0 {
            self.jobs.queue_asset_empty_trash().await?;
        }
        Ok(TrashResponse { count })
    }

    pub async fn restore(&self, auth: &AuthDto) -> Result<TrashResponse, ErrorResp> {
        let count = trash::restore_all_for_user(&self.pool, &auth.user.id).await?;
        Ok(TrashResponse { count })
    }

    pub async fn restore_assets(
        &self,
        auth: &AuthDto,
        dto: &BulkIdsReq,
    ) -> Result<TrashResponse, ErrorResp> {
        if dto.ids.is_empty() {
            return Ok(TrashResponse { count: 0 });
        }

        require_assets_access(&self.pool, auth, &dto.ids, Permission::AssetDelete).await?;
        trash::restore_by_ids(&self.pool, &dto.ids).await?;

        let ids: Vec<String> = dto.ids.iter().map(|id| id.to_string()).collect();
        self.websocket.emit_asset_restore(auth.user.id, ids);

        Ok(TrashResponse {
            count: dto.ids.len() as u64,
        })
    }
}
