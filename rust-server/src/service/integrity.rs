use std::collections::HashMap;
use std::path::Path;

use axum::body::Body;
use axum::http::{header, Response};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::assets;
use crate::models::db::integrity;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::websocket::WebSocketHub;
use crate::utils::file_response::{file_response, FileResponse};
use crate::utils::permission::require_admin;

pub const INTEGRITY_TYPE_UNTRACKED: &str = "untracked_file";
pub const INTEGRITY_TYPE_MISSING: &str = "missing_file";
pub const INTEGRITY_TYPE_CHECKSUM: &str = "checksum_mismatch";

fn parse_report_type(value: &str) -> Result<&'static str, ErrorResp> {
    match value {
        INTEGRITY_TYPE_UNTRACKED => Ok(INTEGRITY_TYPE_UNTRACKED),
        INTEGRITY_TYPE_MISSING => Ok(INTEGRITY_TYPE_MISSING),
        INTEGRITY_TYPE_CHECKSUM => Ok(INTEGRITY_TYPE_CHECKSUM),
        _ => Err(ErrorResp::BadRequest(format!("Invalid integrity report type: {value}"))),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityReportSummaryResponse {
    pub untracked_file: i64,
    pub missing_file: i64,
    pub checksum_mismatch: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityReportItem {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub report_type: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityReportResponse {
    pub items: Vec<IntegrityReportItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<Uuid>,
}

#[derive(Clone)]
pub struct IntegrityService {
    pool: PgPool,
    websocket: WebSocketHub,
}

impl IntegrityService {
    pub fn new(pool: PgPool, websocket: WebSocketHub) -> Self {
        Self { pool, websocket }
    }

    pub async fn get_summary(&self, auth: &AuthDto) -> Result<IntegrityReportSummaryResponse, ErrorResp> {
        require_admin(auth)?;
        let rows = integrity::get_summary(&self.pool)
            .await
            .map_err(ErrorResp::from)?;

        let mut counts: HashMap<String, i64> = HashMap::new();
        for row in rows {
            counts.insert(row.report_type, row.count);
        }

        Ok(IntegrityReportSummaryResponse {
            untracked_file: counts.get(INTEGRITY_TYPE_UNTRACKED).copied().unwrap_or(0),
            missing_file: counts.get(INTEGRITY_TYPE_MISSING).copied().unwrap_or(0),
            checksum_mismatch: counts.get(INTEGRITY_TYPE_CHECKSUM).copied().unwrap_or(0),
        })
    }

    pub async fn get_report(
        &self,
        auth: &AuthDto,
        report_type: &str,
        cursor: Option<Uuid>,
        limit: Option<i64>,
    ) -> Result<IntegrityReportResponse, ErrorResp> {
        require_admin(auth)?;
        let report_type = parse_report_type(report_type)?;
        let limit = limit.unwrap_or(100).clamp(1, 1000);

        let rows = integrity::get_report_page(&self.pool, report_type, cursor, limit + 1)
            .await
            .map_err(ErrorResp::from)?;

        let has_more = rows.len() as i64 > limit;
        let items: Vec<IntegrityReportItem> = rows
            .into_iter()
            .take(limit as usize)
            .map(|row| IntegrityReportItem {
                id: row.id,
                report_type: row.report_type,
                path: row.path,
            })
            .collect();

        let next_cursor = if has_more {
            items.last().map(|item| item.id)
        } else {
            None
        };

        Ok(IntegrityReportResponse {
            items,
            next_cursor,
        })
    }

    pub async fn get_report_file(
        &self,
        auth: &AuthDto,
        id: &Uuid,
    ) -> Result<Response<Body>, ErrorResp> {
        require_admin(auth)?;
        let row = integrity::get_by_id(&self.pool, id)
            .await
            .map_err(|_| ErrorResp::BadRequest("Integrity report not found".to_string()))?;

        let file_name = Path::new(&row.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file")
            .to_string();

        file_response(FileResponse {
            path: row.path,
            content_type: "application/octet-stream".to_string(),
            file_name: Some(file_name),
            cache_control: Some("private, no-cache, no-transform".to_string()),
        })
        .await
    }

    pub async fn delete_report(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        require_admin(auth)?;
        let row = integrity::get_by_id(&self.pool, id)
            .await
            .map_err(|_| ErrorResp::BadRequest("Integrity report not found".to_string()))?;

        if let Some(asset_id) = row.asset_id {
            assets::trash_assets(&self.pool, &[asset_id], false)
                .await
                .map_err(ErrorResp::from)?;
            self.websocket
                .emit_asset_trash(auth.user.id, vec![asset_id.to_string()]);
            integrity::delete_by_id(&self.pool, id)
                .await
                .map_err(ErrorResp::from)?;
        } else if let Some(file_asset_id) = row.file_asset_id {
            integrity::delete_asset_file(&self.pool, &file_asset_id)
                .await
                .map_err(ErrorResp::from)?;
        } else {
            let tracked = integrity::get_tracked_paths(&self.pool, &[row.path.clone()])
                .await
                .map_err(ErrorResp::from)?;
            if tracked.is_empty() {
                let _ = tokio::fs::remove_file(&row.path).await;
            }
            integrity::delete_by_id(&self.pool, id)
                .await
                .map_err(ErrorResp::from)?;
        }

        Ok(())
    }

    pub async fn get_report_csv(
        &self,
        auth: &AuthDto,
        report_type: &str,
    ) -> Result<Response<Body>, ErrorResp> {
        require_admin(auth)?;
        let report_type = parse_report_type(report_type)?;
        let rows = integrity::stream_report_rows(&self.pool, report_type)
            .await
            .map_err(ErrorResp::from)?;

        let mut csv = String::from("id,type,assetId,fileAssetId,path\n");
        for row in rows {
            let escaped_path = row.path.replace('"', "\"\"");
            csv.push_str(&format!(
                "{},{},{},{},\"{escaped_path}\"\n",
                row.id,
                row.report_type,
                row.asset_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                row.file_asset_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "null".to_string()),
            ));
        }

        let filename = format!("{}-{}.csv", chrono::Utc::now().timestamp_millis(), report_type);
        Response::builder()
            .header(header::CONTENT_TYPE, "text/csv")
            .header(header::CACHE_CONTROL, "private, no-cache, no-transform")
            .header(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", urlencoding::encode(&filename)),
            )
            .body(Body::from(csv))
            .map_err(|err| ErrorResp::ServerError(err.to_string()))
    }
}
