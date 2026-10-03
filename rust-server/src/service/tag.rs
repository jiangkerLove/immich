use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::models::db::assets;
use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::access::require_assets_access;
use crate::service::album::{BulkIdErrorReason, BulkIdResponse, BulkIdsReq};
use crate::service::db::DbService;
use crate::service::job::JobService;
use crate::utils::permission::require_permission;

#[derive(Clone)]
pub struct TagService {
    db: DbService,
    jobs: JobService,
}

#[derive(Debug, Serialize, FromRow, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TagResponse {
    pub id: Uuid,
    pub value: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub color: Option<String>,
    pub parent_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCreateReq {
    pub name: String,
    pub color: Option<String>,
    pub parent_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagUpdateReq {
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<Option<String>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagUpsertReq {
    pub tags: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagBulkAssetsReq {
    pub tag_ids: Vec<Uuid>,
    pub asset_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagBulkAssetsResponse {
    pub count: i64,
}

impl TagService {
    pub fn new(pool: sqlx::PgPool, jobs: JobService) -> Self {
        Self {
            db: DbService::new(pool),
            jobs,
        }
    }

    pub async fn get_all(&self, auth: &AuthDto) -> Result<Vec<TagResponse>, ErrorResp> {
        require_permission(auth, Permission::TagRead)?;
        sqlx::query_as::<_, TagResponse>(
            r#"
                SELECT id, value, "createdAt" as created_at, "updatedAt" as updated_at,
                       color, "parentId" as parent_id
                FROM tag
                WHERE "userId" = $1
                ORDER BY value
            "#,
        )
        .bind(auth.user.id)
        .fetch_all(&self.db.pool)
        .await
        .map_err(ErrorResp::from)
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<TagResponse, ErrorResp> {
        require_permission(auth, Permission::TagRead)?;
        self.get_owned(auth, id, Permission::TagRead).await
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &TagCreateReq,
    ) -> Result<TagResponse, ErrorResp> {
        require_permission(auth, Permission::TagCreate)?;

        let value = if let Some(parent_id) = dto.parent_id {
            let parent = self
                .get_owned(auth, &parent_id, Permission::TagRead)
                .await?;
            format!("{}/{}", parent.value, dto.name)
        } else {
            dto.name.clone()
        };

        let duplicate: Option<Uuid> =
            sqlx::query_scalar(r#"SELECT id FROM tag WHERE "userId" = $1 AND value = $2"#)
                .bind(auth.user.id)
                .bind(&value)
                .fetch_optional(&self.db.pool)
                .await?;

        if duplicate.is_some() {
            return Err(ErrorResp::BadRequest(
                "A tag with that name already exists".to_string(),
            ));
        }

        sqlx::query_as::<_, TagResponse>(
            r#"
                INSERT INTO tag ("userId", color, value, "parentId")
                VALUES ($1, $2, $3, $4)
                RETURNING id, value, "createdAt" as created_at, "updatedAt" as updated_at,
                          color, "parentId" as parent_id
            "#,
        )
        .bind(auth.user.id)
        .bind(&dto.color)
        .bind(&value)
        .bind(dto.parent_id)
        .fetch_one(&self.db.pool)
        .await
        .map_err(ErrorResp::from)
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &TagUpdateReq,
    ) -> Result<TagResponse, ErrorResp> {
        require_permission(auth, Permission::TagUpdate)?;
        let existing = self.get_owned(auth, id, Permission::TagUpdate).await?;
        let value = renamed_tag_value(&existing.value, dto.name.as_deref());
        let (set_color, color) = match &dto.color {
            Some(value) => (true, value.clone()),
            None => (false, None),
        };

        sqlx::query_as::<_, TagResponse>(
            r#"
                UPDATE tag
                SET value = $1,
                    color = CASE WHEN $2 THEN $3 ELSE color END
                WHERE id = $4
                RETURNING id, value, "createdAt" as created_at, "updatedAt" as updated_at,
                          color, "parentId" as parent_id
            "#,
        )
        .bind(&value)
        .bind(set_color)
        .bind(color)
        .bind(id)
        .fetch_one(&self.db.pool)
        .await
        .map_err(ErrorResp::from)
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::TagDelete)?;
        self.get_owned(auth, id, Permission::TagDelete).await?;
        sqlx::query(r#"DELETE FROM tag WHERE id = $1"#)
            .bind(id)
            .execute(&self.db.pool)
            .await?;
        Ok(())
    }

    async fn owned_tag_ids(&self, auth: &AuthDto, ids: &[Uuid]) -> Result<Vec<Uuid>, ErrorResp> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        sqlx::query_scalar(r#"SELECT id FROM tag WHERE id = ANY($1) AND "userId" = $2"#)
            .bind(ids)
            .bind(auth.user.id)
            .fetch_all(&self.db.pool)
            .await
            .map_err(ErrorResp::from)
    }

    async fn get_owned(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        permission: Permission,
    ) -> Result<TagResponse, ErrorResp> {
        sqlx::query_as::<_, TagResponse>(
            r#"
                SELECT id, value, "createdAt" as created_at, "updatedAt" as updated_at,
                       color, "parentId" as parent_id
                FROM tag
                WHERE id = $1 AND "userId" = $2
            "#,
        )
        .bind(id)
        .bind(auth.user.id)
        .fetch_optional(&self.db.pool)
        .await?
        .ok_or_else(|| {
            ErrorResp::BadRequest(format!("Not found or no {} access", permission.as_str()))
        })
    }

    pub async fn upsert(
        &self,
        auth: &AuthDto,
        dto: &TagUpsertReq,
    ) -> Result<Vec<TagResponse>, ErrorResp> {
        require_permission(auth, Permission::TagCreate)?;

        let mut unique = std::collections::HashSet::new();
        let mut results = Vec::new();
        for tag in &dto.tags {
            if !unique.insert(tag.clone()) {
                continue;
            }
            let parts: Vec<&str> = tag.split('/').filter(|part| !part.is_empty()).collect();
            let mut parent: Option<TagResponse> = None;
            for part in parts {
                let value = if let Some(parent_tag) = &parent {
                    format!("{}/{}", parent_tag.value, part)
                } else {
                    part.to_string()
                };
                parent = Some(
                    self.upsert_value(auth, &value, parent.as_ref().map(|p| p.id))
                        .await?,
                );
            }
            if let Some(tag) = parent {
                results.push(tag);
            }
        }
        Ok(results)
    }

    pub async fn bulk_tag_assets(
        &self,
        auth: &AuthDto,
        dto: &TagBulkAssetsReq,
    ) -> Result<TagBulkAssetsResponse, ErrorResp> {
        require_permission(auth, Permission::TagAsset)?;
        let elevated = auth
            .session
            .as_ref()
            .is_some_and(|session| session.has_elevated_permission);
        let asset_ids = assets::filter_accessible_ids(
            &self.db.pool,
            &auth.user.id,
            &dto.asset_ids,
            elevated,
            true,
        )
        .await?;
        let tag_ids = self.owned_tag_ids(auth, &dto.tag_ids).await?;

        let mut count = 0i64;
        let mut touched = std::collections::HashSet::new();
        for tag_id in &tag_ids {
            for asset_id in &asset_ids {
                let inserted = sqlx::query(
                    r#"
                        INSERT INTO tag_asset ("tagId", "assetId")
                        VALUES ($1, $2)
                        ON CONFLICT DO NOTHING
                    "#,
                )
                .bind(tag_id)
                .bind(asset_id)
                .execute(&self.db.pool)
                .await?;
                let rows = inserted.rows_affected() as i64;
                count += rows;
                if rows > 0 {
                    touched.insert(*asset_id);
                }
            }
        }
        for asset_id in touched {
            self.sync_asset_tags(&asset_id).await?;
            self.jobs.queue_sidecar_write(&asset_id).await?;
            self.trigger_asset_tagged(&auth.user.id, &asset_id).await;
        }
        Ok(TagBulkAssetsResponse { count })
    }

    pub async fn add_assets(
        &self,
        auth: &AuthDto,
        tag_id: &Uuid,
        dto: &BulkIdsReq,
    ) -> Result<Vec<BulkIdResponse>, ErrorResp> {
        require_permission(auth, Permission::TagAsset)?;
        self.get_owned(auth, tag_id, Permission::TagAsset).await?;

        let mut results = Vec::new();
        for asset_id in &dto.ids {
            let already_tagged: bool = sqlx::query_scalar(
                r#"SELECT EXISTS(SELECT 1 FROM tag_asset WHERE "tagId" = $1 AND "assetId" = $2)"#,
            )
            .bind(tag_id)
            .bind(asset_id)
            .fetch_one(&self.db.pool)
            .await
            .unwrap_or(false);
            if already_tagged {
                results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::Duplicate),
                    error_message: None,
                });
                continue;
            }

            match require_assets_access(&self.db.pool, auth, &[*asset_id], Permission::AssetUpdate)
                .await
            {
                Ok(()) => {
                    let _ = sqlx::query(
                        r#"INSERT INTO tag_asset ("tagId", "assetId") VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
                    )
                    .bind(tag_id)
                    .bind(asset_id)
                    .execute(&self.db.pool)
                    .await;
                    self.sync_asset_tags(asset_id).await?;
                    self.jobs.queue_sidecar_write(asset_id).await?;
                    self.trigger_asset_tagged(&auth.user.id, asset_id).await;
                    results.push(BulkIdResponse {
                        id: *asset_id,
                        success: true,
                        error: None,
                        error_message: None,
                    });
                }
                Err(_) => results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NoPermission),
                    error_message: None,
                }),
            }
        }
        Ok(results)
    }

    pub async fn remove_assets(
        &self,
        auth: &AuthDto,
        tag_id: &Uuid,
        dto: &BulkIdsReq,
    ) -> Result<Vec<BulkIdResponse>, ErrorResp> {
        require_permission(auth, Permission::TagAsset)?;
        self.get_owned(auth, tag_id, Permission::TagAsset).await?;

        let existing: std::collections::HashSet<Uuid> = sqlx::query_scalar(
            r#"SELECT "assetId" FROM tag_asset WHERE "tagId" = $1 AND "assetId" = ANY($2)"#,
        )
        .bind(tag_id)
        .bind(&dto.ids)
        .fetch_all(&self.db.pool)
        .await?
        .into_iter()
        .collect();

        let can_always_remove = require_permission(auth, Permission::TagDelete).is_ok();
        let allowed: std::collections::HashSet<Uuid> = if can_always_remove {
            existing.clone()
        } else {
            let asset_list: Vec<Uuid> = existing.iter().copied().collect();
            assets::filter_shareable_ids(&self.db.pool, &auth.user.id, &asset_list)
                .await?
                .into_iter()
                .collect()
        };

        let mut results = Vec::new();
        for asset_id in &dto.ids {
            if !existing.contains(asset_id) {
                results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NotFound),
                    error_message: None,
                });
                continue;
            }
            if !allowed.contains(asset_id) {
                results.push(BulkIdResponse {
                    id: *asset_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NoPermission),
                    error_message: None,
                });
                continue;
            }
            sqlx::query(r#"DELETE FROM tag_asset WHERE "tagId" = $1 AND "assetId" = $2"#)
                .bind(tag_id)
                .bind(asset_id)
                .execute(&self.db.pool)
                .await?;
            self.sync_asset_tags(asset_id).await?;
            self.jobs.queue_sidecar_write(asset_id).await?;
            results.push(BulkIdResponse {
                id: *asset_id,
                success: true,
                error: None,
                error_message: None,
            });
        }
        Ok(results)
    }

    async fn upsert_value(
        &self,
        auth: &AuthDto,
        value: &str,
        parent_id: Option<Uuid>,
    ) -> Result<TagResponse, ErrorResp> {
        if let Some(existing) = sqlx::query_as::<_, TagResponse>(
            r#"
                SELECT id, value, "createdAt" as created_at, "updatedAt" as updated_at,
                       color, "parentId" as parent_id
                FROM tag WHERE "userId" = $1 AND value = $2
            "#,
        )
        .bind(auth.user.id)
        .bind(value)
        .fetch_optional(&self.db.pool)
        .await?
        {
            return Ok(existing);
        }

        sqlx::query_as::<_, TagResponse>(
            r#"
                INSERT INTO tag ("userId", value, "parentId")
                VALUES ($1, $2, $3)
                RETURNING id, value, "createdAt" as created_at, "updatedAt" as updated_at,
                          color, "parentId" as parent_id
            "#,
        )
        .bind(auth.user.id)
        .bind(value)
        .bind(parent_id)
        .fetch_one(&self.db.pool)
        .await
        .map_err(ErrorResp::from)
    }

    async fn sync_asset_tags(&self, asset_id: &Uuid) -> Result<(), ErrorResp> {
        let tag_values: Vec<String> = sqlx::query_scalar(
            r#"
                SELECT t.value
                FROM tag t
                INNER JOIN tag_asset ta ON ta."tagId" = t.id
                WHERE ta."assetId" = $1
                ORDER BY t.value ASC
            "#,
        )
        .bind(asset_id)
        .fetch_all(&self.db.pool)
        .await?;

        let _ = sqlx::query(r#"UPDATE asset_exif SET tags = $1 WHERE "assetId" = $2"#)
            .bind(&tag_values)
            .bind(asset_id)
            .execute(&self.db.pool)
            .await;
        // Match TS TagService.updateTags → updateLockedColumns({ tags }) append.
        let _ = assets::append_locked_properties(&self.db.pool, &[*asset_id], &["tags"]).await;
        Ok(())
    }

    async fn trigger_asset_tagged(&self, user_id: &Uuid, asset_id: &Uuid) {
        let _ = crate::service::workflow_trigger::on_asset_trigger(
            &self.db.pool,
            &self.jobs,
            user_id,
            asset_id,
            crate::utils::workflow::TRIGGER_ASSET_TAGGED,
        )
        .await;
    }
}

fn renamed_tag_value(existing: &str, name: Option<&str>) -> String {
    let Some(name) = name.filter(|name| !name.is_empty()) else {
        return existing.to_string();
    };
    let mut parts: Vec<&str> = existing.split('/').collect();
    if let Some(last) = parts.last_mut() {
        *last = name;
    }
    parts.join("/")
}
