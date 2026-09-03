use std::path::Path;

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::asset_ocr::{self, OcrInsertRow};
use crate::models::db::ml_job;
use crate::models::db::system_metadata::{get_machine_learning_config, is_ocr_enabled};
use crate::service::job::JobService;
use crate::service::ml;
use crate::utils::search::tokenize_for_search;

const JOBS_BATCH_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrOutcome {
    Skipped,
    Failed,
    Success,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrQueueAllOutcome {
    Skipped,
    Success,
}

#[derive(Clone)]
pub struct OcrService {
    pool: PgPool,
    jobs: JobService,
}

impl OcrService {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self { pool, jobs }
    }

    pub async fn queue_all(&self, force: bool) -> Result<OcrQueueAllOutcome, String> {
        let config = get_machine_learning_config(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if !is_ocr_enabled(&config) {
            return Ok(OcrQueueAllOutcome::Skipped);
        }

        if force {
            asset_ocr::delete_all(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
        }

        let asset_ids = ml_job::stream_for_ocr(&self.pool, force)
            .await
            .map_err(|err| err.to_string())?;

        for chunk in asset_ids.chunks(JOBS_BATCH_SIZE) {
            for asset_id in chunk {
                self.jobs
                    .queue_ocr(asset_id, None)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        Ok(OcrQueueAllOutcome::Success)
    }

    pub async fn process_asset(&self, asset_id: &Uuid) -> Result<OcrOutcome, String> {
        let config = get_machine_learning_config(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if !is_ocr_enabled(&config) {
            return Ok(OcrOutcome::Skipped);
        }

        let Some(asset) = ml_job::get_for_ocr(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(OcrOutcome::Failed);
        };

        if asset.visibility == "hidden" {
            return Ok(OcrOutcome::Skipped);
        }

        let Some(preview_path) = asset.preview_path else {
            return Ok(OcrOutcome::Failed);
        };

        if !Path::new(&preview_path).exists() {
            return Ok(OcrOutcome::Failed);
        }

        let ocr = ml::run_ocr(&config, Path::new(&preview_path), &config.ocr)
            .await
            .map_err(|err| err.to_string())?;

        let mut rows = Vec::new();
        let mut search_tokens = Vec::new();
        for (index, text) in ocr.text.iter().enumerate() {
            let offset = index * 8;
            if offset + 7 >= ocr.r#box.len() {
                break;
            }
            rows.push(OcrInsertRow {
                asset_id: *asset_id,
                x1: ocr.r#box[offset],
                y1: ocr.r#box[offset + 1],
                x2: ocr.r#box[offset + 2],
                y2: ocr.r#box[offset + 3],
                x3: ocr.r#box[offset + 4],
                y3: ocr.r#box[offset + 5],
                x4: ocr.r#box[offset + 6],
                y4: ocr.r#box[offset + 7],
                box_score: ocr.box_score.get(index).copied().unwrap_or(0.0),
                text_score: ocr.text_score.get(index).copied().unwrap_or(0.0),
                text: text.clone(),
            });
            search_tokens.extend(tokenize_for_search(text));
        }

        asset_ocr::upsert_for_asset(&self.pool, asset_id, &rows, &search_tokens.join(" "))
            .await
            .map_err(|err| err.to_string())?;
        ml_job::set_ocr_at(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?;

        Ok(OcrOutcome::Success)
    }
}
