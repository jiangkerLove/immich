use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::auth_permission::Permission;
use crate::models::db::cluster_group::{self, ClusterGroupRequestRow, ClusterGroupUserRow};
use crate::models::db::person;
use crate::models::db::users::UserDb;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserResponse;
use crate::service::job::JobService;
use crate::utils::permission::require_permission;

#[derive(Clone)]
pub struct ClusterGroupService {
    pool: PgPool,
    jobs: JobService,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterGroupRequestResponse {
    pub id: Uuid,
    pub cluster_group_id: Uuid,
    pub user_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, serde::Serialize)]
pub struct ClusterGroupRequestCreateResult {
    pub duplicate: bool,
    pub value: ClusterGroupRequestResponse,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterGroupRequestCreateReq {
    pub user_id: Uuid,
}

impl ClusterGroupService {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self { pool, jobs }
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

    pub async fn create_request(
        &self,
        auth: &AuthDto,
        cluster_group_id: &Uuid,
        user_id: &Uuid,
    ) -> Result<ClusterGroupRequestCreateResult, ErrorResp> {
        require_permission(auth, Permission::ClusterGroupRequestCreate)?;
        self.require_tables().await?;
        self.require_cluster_group_owner(auth, cluster_group_id).await?;

        if user_id == &auth.user.id {
            return Err(ErrorResp::BadRequest(
                "Cannot request to join your own cluster group".to_string(),
            ));
        }

        if !cluster_group::user_exists(&self.pool, user_id)
            .await
            .map_err(ErrorResp::from)?
        {
            return Err(ErrorResp::NotFound("User not found".to_string()));
        }

        let result = cluster_group::create_request(&self.pool, cluster_group_id, user_id)
            .await
            .map_err(ErrorResp::from)?;

        Ok(ClusterGroupRequestCreateResult {
            duplicate: !result.is_inserted,
            value: map_request(result.row),
        })
    }

    pub async fn accept_request(&self, auth: &AuthDto, request_id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::ClusterGroupRequestRead)?;
        self.require_tables().await?;
        self.require_request_read(auth, request_id).await?;

        let request = cluster_group::get_request(&self.pool, request_id)
            .await
            .map_err(ErrorResp::from)?
            .ok_or_else(|| ErrorResp::NotFound("Request not found".to_string()))?;

        person::reassign_cluster(&self.pool, &auth.user.id, &request.cluster_group_id)
            .await
            .map_err(ErrorResp::from)?;
        UserDb::update_cluster_group_id(&self.pool, &auth.user.id, &request.cluster_group_id)
            .await
            .map_err(ErrorResp::from)?;
        cluster_group::delete_request(&self.pool, request_id)
            .await
            .map_err(ErrorResp::from)?;

        Ok(())
    }

    pub async fn delete_request(&self, auth: &AuthDto, request_id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::ClusterGroupRequestDelete)?;
        self.require_tables().await?;
        self.require_request_delete(auth, request_id).await?;

        cluster_group::delete_request(&self.pool, request_id)
            .await
            .map_err(ErrorResp::from)?;

        Ok(())
    }

    pub async fn regenerate_people(
        &self,
        auth: &AuthDto,
        cluster_group_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::ClusterGroupRead)?;
        self.require_tables().await?;
        self.require_cluster_group_read(auth, cluster_group_id).await?;

        self.jobs
            .queue_facial_recognition_queue_all(true, Some(*cluster_group_id))
            .await?;

        Ok(())
    }

    pub async fn leave(&self, auth: &AuthDto, cluster_group_id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::ClusterGroupLeave)?;
        self.require_tables().await?;
        self.require_cluster_group_owner(auth, cluster_group_id).await?;

        if !cluster_group::has_other_members(&self.pool, cluster_group_id, &auth.user.id)
            .await
            .map_err(ErrorResp::from)?
        {
            return Err(ErrorResp::BadRequest(
                "Cannot leave a cluster group without any other members".to_string(),
            ));
        }

        let new_cluster_id = cluster_group::create(&self.pool)
            .await
            .map_err(ErrorResp::from)?;
        person::reassign_cluster(&self.pool, &auth.user.id, &new_cluster_id)
            .await
            .map_err(ErrorResp::from)?;
        UserDb::update_cluster_group_id(&self.pool, &auth.user.id, &new_cluster_id)
            .await
            .map_err(ErrorResp::from)?;

        Ok(())
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

    async fn require_cluster_group_owner(
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
            return Err(ErrorResp::BadRequest(
                "Not found or no clusterGroup access".to_string(),
            ));
        }

        Ok(())
    }

    async fn require_request_read(
        &self,
        auth: &AuthDto,
        request_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        if !cluster_group::request_owned_by_user(&self.pool, request_id, &auth.user.id)
            .await
            .map_err(ErrorResp::from)?
        {
            return Err(ErrorResp::BadRequest(
                "Not found or no clusterGroupRequest.read access".to_string(),
            ));
        }
        Ok(())
    }

    async fn require_request_delete(
        &self,
        auth: &AuthDto,
        request_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        if !cluster_group::can_delete_request(&self.pool, request_id, &auth.user.id)
            .await
            .map_err(ErrorResp::from)?
        {
            return Err(ErrorResp::BadRequest(
                "Not found or no clusterGroupRequest.delete access".to_string(),
            ));
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
