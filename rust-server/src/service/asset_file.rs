use std::path::Path;

use axum::body::Body;
use axum::http::Response;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::asset_file::{self, AssetFileRow, AssetFileSearchFilter};
use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::access::require_asset_access;
use crate::utils::file_response::{file_response, FileResponse};
use crate::utils::permission::require_permission;

#[derive(Clone)]
pub struct AssetFileService {
    pool: PgPool,
    jobs: crate::service::job::JobService,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetFileSearchQuery {
    pub asset_id: Uuid,
    pub r#type: Option<String>,
    pub is_edited: Option<String>,
    pub is_progressive: Option<String>,
    pub is_transparent: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetFileResponse {
    pub id: Uuid,
    pub created_at: String,
    pub updated_at: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub path: String,
    pub is_edited: bool,
    pub is_progressive: bool,
    pub is_transparent: bool,
}

impl AssetFileService {
    pub fn new(pool: PgPool, jobs: crate::service::job::JobService) -> Self {
        Self { pool, jobs }
    }

    pub async fn search(
        &self,
        auth: &AuthDto,
        query: &AssetFileSearchQuery,
    ) -> Result<Vec<AssetFileResponse>, ErrorResp> {
        require_permission(auth, Permission::AssetFileRead)?;
        require_asset_access(&self.pool, auth, &query.asset_id, Permission::AssetRead).await?;

        let rows = asset_file::search(
            &self.pool,
            &AssetFileSearchFilter {
                asset_id: query.asset_id,
                file_type: query.r#type.clone(),
                is_edited: parse_bool(&query.is_edited),
                is_progressive: parse_bool(&query.is_progressive),
                is_transparent: parse_bool(&query.is_transparent),
            },
        )
        .await?;

        Ok(rows.into_iter().map(map_asset_file).collect())
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<AssetFileResponse, ErrorResp> {
        require_permission(auth, Permission::AssetFileRead)?;
        self.require_file_access(auth, id).await?;

        let row = asset_file::get_by_id(&self.pool, id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Asset file not found".to_string()))?;

        Ok(map_asset_file(row))
    }

    pub async fn download(
        &self,
        auth: &AuthDto,
        id: &Uuid,
    ) -> Result<Response<Body>, ErrorResp> {
        require_permission(auth, Permission::AssetFileDownload)?;
        self.require_file_access(auth, id).await?;

        let row = asset_file::get_by_id(&self.pool, id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Asset file not found".to_string()))?;

        let file_name = Path::new(&row.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file")
            .to_string();

        file_response(FileResponse {
            path: row.path,
            content_type: mime_guess::from_path(&file_name)
                .first_or_octet_stream()
                .to_string(),
            file_name: Some(file_name),
            cache_control: Some("private".to_string()),
        })
        .await
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::AssetFileDelete)?;
        self.require_file_access(auth, id).await?;

        let row = asset_file::get_by_id(&self.pool, id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Asset file not found".to_string()))?;

        if row.file_type == "sidecar" {
            return Err(ErrorResp::BadRequest(
                "Sidecar files cannot be deleted".to_string(),
            ));
        }

        asset_file::delete_by_id(&self.pool, id).await?;
        self.jobs.queue_file_delete(&[row.path]).await?;
        Ok(())
    }

    async fn require_file_access(&self, auth: &AuthDto, file_id: &Uuid) -> Result<(), ErrorResp> {
        let elevated = auth
            .session
            .as_ref()
            .is_some_and(|session| session.has_elevated_permission);
        let allowed =
            asset_file::filter_owner_accessible_ids(&self.pool, &auth.user.id, &[*file_id], elevated)
                .await?;
        if allowed.is_empty() {
            return Err(ErrorResp::BadRequest(
                "Not found or no assetFile.read access".to_string(),
            ));
        }
        Ok(())
    }
}

fn map_asset_file(row: AssetFileRow) -> AssetFileResponse {
    AssetFileResponse {
        id: row.id,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
        file_type: row.file_type,
        path: row.path,
        is_edited: row.is_edited,
        is_progressive: row.is_progressive,
        is_transparent: row.is_transparent,
    }
}

fn parse_bool(value: &Option<String>) -> Option<bool> {
    value
        .as_deref()
        .and_then(crate::utils::query::parse_query_bool)
}
