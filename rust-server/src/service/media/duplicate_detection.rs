use std::collections::HashSet;

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::ml_job::{self, DuplicateMatchRow, DuplicateSearchAssetRow};
use crate::models::db::system_metadata::{
    get_machine_learning_config, is_duplicate_detection_enabled,
};
use crate::service::job::JobService;

const JOBS_BATCH_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateDetectionOutcome {
    Skipped,
    Failed,
    Success,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateDetectionQueueAllOutcome {
    Skipped,
    Success,
}

#[derive(Clone)]
pub struct DuplicateDetectionService {
    pool: PgPool,
    jobs: JobService,
}

impl DuplicateDetectionService {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self { pool, jobs }
    }

    pub async fn queue_all(
        &self,
        force: bool,
    ) -> Result<DuplicateDetectionQueueAllOutcome, String> {
        let config = get_machine_learning_config(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if !is_duplicate_detection_enabled(&config) {
            return Ok(DuplicateDetectionQueueAllOutcome::Skipped);
        }

        let asset_ids = ml_job::stream_for_duplicate_search(&self.pool, force)
            .await
            .map_err(|err| err.to_string())?;

        for chunk in asset_ids.chunks(JOBS_BATCH_SIZE) {
            for asset_id in chunk {
                self.jobs
                    .queue_asset_detect_duplicates(asset_id, None)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        Ok(DuplicateDetectionQueueAllOutcome::Success)
    }

    pub async fn detect_asset(&self, asset_id: &Uuid) -> Result<DuplicateDetectionOutcome, String> {
        let config = get_machine_learning_config(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if !is_duplicate_detection_enabled(&config) {
            return Ok(DuplicateDetectionOutcome::Skipped);
        }

        let Some(asset) = ml_job::get_for_duplicate_search(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(DuplicateDetectionOutcome::Failed);
        };

        if asset.stack_id.is_some() {
            return Ok(DuplicateDetectionOutcome::Skipped);
        }

        if asset.visibility == "hidden" || asset.visibility == "locked" {
            return Ok(DuplicateDetectionOutcome::Skipped);
        }

        let Some(ref embedding) = asset.embedding else {
            return Ok(DuplicateDetectionOutcome::Failed);
        };

        let duplicate_assets = ml_job::search_duplicate_assets(
            &self.pool,
            asset_id,
            embedding,
            config.duplicate_detection.max_distance,
            &asset.asset_type,
            &[asset.owner_id],
        )
        .await
        .map_err(|err| err.to_string())?;

        let mut asset_ids = vec![*asset_id];
        if !duplicate_assets.is_empty() {
            asset_ids = self.update_duplicates(&asset, &duplicate_assets).await?;
        } else if asset.duplicate_id.is_some() {
            ml_job::clear_duplicate_id(&self.pool, asset_id)
                .await
                .map_err(|err| err.to_string())?;
        }

        ml_job::set_duplicates_detected_at(&self.pool, &asset_ids)
            .await
            .map_err(|err| err.to_string())?;

        Ok(DuplicateDetectionOutcome::Success)
    }

    async fn update_duplicates(
        &self,
        asset: &DuplicateSearchAssetRow,
        duplicate_assets: &[DuplicateMatchRow],
    ) -> Result<Vec<Uuid>, String> {
        let mut duplicate_ids: HashSet<Uuid> = duplicate_assets
            .iter()
            .filter_map(|row| row.duplicate_id)
            .collect();

        let target_duplicate_id = asset.duplicate_id.unwrap_or_else(|| {
            duplicate_ids
                .iter()
                .copied()
                .next()
                .unwrap_or_else(Uuid::new_v4)
        });

        let source_ids: Vec<Uuid> = if asset.duplicate_id.is_some() {
            duplicate_ids.into_iter().collect()
        } else {
            duplicate_ids.remove(&target_duplicate_id);
            duplicate_ids.into_iter().collect()
        };

        let mut asset_ids_to_update: Vec<Uuid> = duplicate_assets
            .iter()
            .filter(|row| row.duplicate_id != Some(target_duplicate_id))
            .map(|row| row.asset_id)
            .collect();
        asset_ids_to_update.push(asset.id);

        ml_job::merge_duplicate_group(
            &self.pool,
            &target_duplicate_id,
            &asset_ids_to_update,
            &source_ids,
        )
        .await
        .map_err(|err| err.to_string())?;

        Ok(asset_ids_to_update)
    }
}
