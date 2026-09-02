use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::auth_permission::Permission;
use crate::models::db::cluster_group::{self, ClusterGroupRequestRow, ClusterGroupUserRow};
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserResponse;
use crate::utils::permission::require_permission;

#[derive(Clone)]
pub struct ClusterGroupService {
    pool: PgPool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterGroupRequestResponse {
    pub id: Uuid,
    pub cluster_group_id: Uuid,
    pub user_id: Uuid,
    pub created_at: DateTime<Utc>,
}

impl ClusterGroupService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_requests(
        &self,
        auth: &AuthDto,
    ) -> Result<Vec<ClusterGroupRequestResponse>, ErrorResp> {
        require_permission(auth, Permission::ClusterGroupRequestRead)?;
        self.require_tables().await?;

        let rows = cluster_group::search_requests(&self.pool, Some(&auth.user.id), None)
            .await
            .map_err(ErrorResp::from)?;

        Ok(rows.into_iter().map(map_request).collect())
    }

    pub async fn get_requests_for_group(
        &self,
        auth: &AuthDto,
        cluster_group_id: &Uuid,
    ) -> Result<Vec<ClusterGroupRequestResponse>, ErrorResp> {
        require_permission(auth, Permission::ClusterGroupRequestRead)?;
        self.require_tables().await?;
        self.require_cluster_group_read(auth, cluster_group_id).await?;

        let rows =
            cluster_group::search_requests(&self.pool, None, Some(cluster_group_id))
                .await
                .map_err(ErrorResp::from)?;

        Ok(rows.into_iter().map(map_request).collect())
    }

    pub async fn get_users(
        &self,
        auth: &AuthDto,
        cluster_group_id: &Uuid,
    ) -> Result<Vec<UserResponse>, ErrorResp> {
        require_permission(auth, Permission::ClusterGroupRead)?;
        self.require_tables().await?;
        self.require_cluster_group_read(auth, cluster_group_id).await?;

        let rows = cluster_group::get_users(&self.pool, cluster_group_id, &auth.user.id)
            .await
            .map_err(ErrorResp::from)?;

        Ok(rows.into_iter().map(map_user).collect())
    }

    async fn require_tables(&self) -> Result<(), ErrorResp> {
        if cluster_group::tables_exist(&self.pool)
            .await
            .map_err(ErrorResp::from)?
        {
            Ok(())
        } else {
            Err(ErrorResp::BadRequest(
                "Cluster groups are not available on this database".to_string(),
            ))
        }
    }

    async fn require_cluster_group_read(
        &self,
        auth: &AuthDto,
        cluster_group_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        let members = cluster_group::member_cluster_group_ids(
            &self.pool,
            &auth.user.id,
            &[*cluster_group_id],
        )
        .await
        .map_err(ErrorResp::from)?;

        if members.is_empty() {
            let invited = cluster_group::invited_cluster_group_ids(
                &self.pool,
                &auth.user.id,
                &[*cluster_group_id],
            )
            .await
            .map_err(ErrorResp::from)?;

            if invited.is_empty() {
                return Err(ErrorResp::BadRequest(
                    "Not found or no clusterGroup.read access".to_string(),
                ));
            }
        }

        Ok(())
    }
}

fn map_request(row: ClusterGroupRequestRow) -> ClusterGroupRequestResponse {
    ClusterGroupRequestResponse {
        id: row.id,
        cluster_group_id: row.cluster_group_id,
        user_id: row.user_id,
        created_at: row.created_at,
    }
}

fn map_user(row: ClusterGroupUserRow) -> UserResponse {
    UserResponse {
        id: row.id.to_string(),
        email: row.email,
        name: row.name,
        profile_image_path: row.profile_image_path,
        avatar_color: row.avatar_color.unwrap_or_default(),
        profile_changed_at: row.profile_changed_at,
    }
}
