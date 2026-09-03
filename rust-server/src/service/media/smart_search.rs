use std::path::Path;

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::smart_search;
use crate::models::db::smart_search_job;
use crate::models::db::system_metadata::{get_machine_learning_config, is_smart_search_enabled};
use crate::service::job::{EntityJob, JobService};
use crate::service::ml;
use crate::utils::clip::get_clip_dim_size;

const JOBS_BATCH_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartSearchOutcome {
    Skipped,
    Failed,
    Success,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartSearchQueueAllOutcome {
    Skipped,
    Success,
}

#[derive(Clone)]
pub struct SmartSearchService {
    pool: PgPool,
    jobs: JobService,
}

impl SmartSearchService {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self { pool, jobs }
    }

    pub async fn queue_all(&self, force: bool) -> Result<SmartSearchQueueAllOutcome, String> {
        let config = get_machine_learning_config(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if !is_smart_search_enabled(&config) {
            return Ok(SmartSearchQueueAllOutcome::Skipped);
        }
        if !crate::utils::vector::smart_search_available(&self.pool).await {
            return Ok(SmartSearchQueueAllOutcome::Skipped);
        }

        if force {
            let dim_size = get_clip_dim_size(&config.clip.model_name)?;
            crate::utils::vector::set_dimension_size(&self.pool, dim_size, None).await?;
        }

        let asset_ids = smart_search_job::stream_for_encode_clip(&self.pool, force)
            .await
            .map_err(|err| err.to_string())?;

        for chunk in asset_ids.chunks(JOBS_BATCH_SIZE) {
            for asset_id in chunk {
                self.jobs
                    .queue_smart_search(asset_id, None)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        Ok(SmartSearchQueueAllOutcome::Success)
    }

    pub async fn encode_asset(
        &self,
        asset_id: &Uuid,
        job: &EntityJob,
    ) -> Result<SmartSearchOutcome, String> {
        let config = get_machine_learning_config(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if !is_smart_search_enabled(&config) {
            return Ok(SmartSearchOutcome::Skipped);
        }

        if !crate::utils::vector::smart_search_available(&self.pool).await {
            return Ok(SmartSearchOutcome::Skipped);
        }

        let model_name = config.clip.model_name.clone();

        let Some(asset) = smart_search_job::get_for_clip_encoding(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(SmartSearchOutcome::Failed);
        };

        if asset.preview_file_count != 1 {
            return Ok(SmartSearchOutcome::Failed);
        }

        let Some(preview_path) = asset.preview_path else {
            return Ok(SmartSearchOutcome::Failed);
        };

        if asset.visibility == "hidden" {
            return Ok(SmartSearchOutcome::Skipped);
        }

        if !Path::new(&preview_path).exists() {
            return Ok(SmartSearchOutcome::Failed);
        }

        let embedding = ml::encode_clip_image(&config, Path::new(&preview_path))
            .await
            .map_err(|err| err.to_string())?;

        let new_config = get_machine_learning_config(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if new_config.clip.model_name != model_name {
            return Ok(SmartSearchOutcome::Skipped);
        }

        smart_search::upsert_embedding(&self.pool, asset_id, &embedding)
            .await
            .map_err(|err| err.to_string())?;

        if job.source.as_deref() == Some("upload") {
            self.jobs
                .queue_asset_detect_duplicates(asset_id, job.source.as_deref())
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(SmartSearchOutcome::Success)
    }
}
