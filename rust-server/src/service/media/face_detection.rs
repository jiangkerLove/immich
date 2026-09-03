use std::collections::HashSet;
use std::path::Path;

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::face::{self, NewMlFace};
use crate::models::db::ml_job::{self, DetectFaceAssetFace};
use crate::models::db::system_metadata::{
    get_machine_learning_config, is_facial_recognition_enabled,
};
use crate::service::job::JobService;
use crate::service::ml;

const JOBS_BATCH_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceDetectionOutcome {
    Skipped,
    Failed,
    Success,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceDetectionQueueAllOutcome {
    Skipped,
    Success,
}

#[derive(Clone)]
pub struct FaceDetectionService {
    pool: PgPool,
    jobs: JobService,
}

impl FaceDetectionService {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self { pool, jobs }
    }

    pub async fn queue_all(
        &self,
        force: Option<bool>,
    ) -> Result<FaceDetectionQueueAllOutcome, String> {
        let config = get_machine_learning_config(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if !is_facial_recognition_enabled(&config) {
            return Ok(FaceDetectionQueueAllOutcome::Skipped);
        }
        if !crate::utils::vector::face_search_available(&self.pool).await {
            return Ok(FaceDetectionQueueAllOutcome::Skipped);
        }

        if force.unwrap_or(false) {
            face::delete_ml_faces(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
            self.jobs
                .queue_person_cleanup()
                .await
                .map_err(|err| err.to_string())?;
            crate::models::db::person::vacuum_faces(&self.pool, true)
                .await
                .map_err(|err| err.to_string())?;
        }

        let asset_ids = ml_job::stream_for_detect_faces(&self.pool, force.unwrap_or(false))
            .await
            .map_err(|err| err.to_string())?;

        for chunk in asset_ids.chunks(JOBS_BATCH_SIZE) {
            for asset_id in chunk {
                self.jobs
                    .queue_asset_detect_faces(asset_id, None)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        if force.is_none() {
            self.jobs
                .queue_person_cleanup()
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(FaceDetectionQueueAllOutcome::Success)
    }

    pub async fn detect_asset(&self, asset_id: &Uuid) -> Result<FaceDetectionOutcome, String> {
        let config = get_machine_learning_config(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if !is_facial_recognition_enabled(&config) {
            return Ok(FaceDetectionOutcome::Skipped);
        }
        if !crate::utils::vector::face_search_available(&self.pool).await {
            return Ok(FaceDetectionOutcome::Skipped);
        }

        let Some(asset) = ml_job::get_for_detect_faces(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(FaceDetectionOutcome::Failed);
        };

        if asset.preview_file_count != 1 {
            return Ok(FaceDetectionOutcome::Failed);
        }

        let Some(preview_path) = asset.preview_path else {
            return Ok(FaceDetectionOutcome::Failed);
        };

        if asset.visibility == "hidden" {
            return Ok(FaceDetectionOutcome::Skipped);
        }

        if !Path::new(&preview_path).exists() {
            return Ok(FaceDetectionOutcome::Failed);
        }

        let existing_faces = parse_existing_faces(asset.faces);
        let detection = ml::detect_faces(
            &config,
            Path::new(&preview_path),
            &config.facial_recognition.model_name,
            config.facial_recognition.min_score,
        )
        .await
        .map_err(|err| err.to_string())?;

        let mut ml_face_ids: HashSet<Uuid> = existing_faces
            .iter()
            .filter(|face| face.source_type == "machine-learning")
            .map(|face| face.id)
            .collect();

        let height_scale = if existing_faces.is_empty() {
            1.0
        } else {
            detection.image_height as f64 / existing_faces[0].image_height as f64
        };
        let width_scale = if existing_faces.is_empty() {
            1.0
        } else {
            detection.image_width as f64 / existing_faces[0].image_width as f64
        };

        let mut faces_to_add = Vec::new();
        let mut face_ids_to_remove = Vec::new();
        let mut new_face_ids = Vec::new();

        for detected in &detection.faces {
            let scaled_box = BoundingBox {
                x1: detected.x1 * width_scale,
                y1: detected.y1 * height_scale,
                x2: detected.x2 * width_scale,
                y2: detected.y2 * height_scale,
            };

            let matched = existing_faces
                .iter()
                .find(|face| iou(face, &scaled_box) > 0.5);

            if let Some(existing) = matched {
                if !ml_face_ids.remove(&existing.id) {
                    face::upsert_face_embedding(&self.pool, &existing.id, &detected.embedding)
                        .await
                        .map_err(|err| err.to_string())?;
                }
            } else {
                let face_id = Uuid::new_v4();
                faces_to_add.push(NewMlFace {
                    id: face_id,
                    asset_id: *asset_id,
                    image_width: detection.image_width,
                    image_height: detection.image_height,
                    bounding_box_x1: detected.x1 as i32,
                    bounding_box_y1: detected.y1 as i32,
                    bounding_box_x2: detected.x2 as i32,
                    bounding_box_y2: detected.y2 as i32,
                    embedding: &detected.embedding,
                });
                new_face_ids.push(face_id);
            }
        }

        face_ids_to_remove.extend(ml_face_ids);

        if !faces_to_add.is_empty() || !face_ids_to_remove.is_empty() {
            face::refresh_ml_faces(&self.pool, &faces_to_add, &face_ids_to_remove)
                .await
                .map_err(|err| err.to_string())?;
        }

        if !new_face_ids.is_empty() {
            self.jobs
                .queue_facial_recognition_queue_all(false, None)
                .await
                .map_err(|err| err.to_string())?;
            for face_id in new_face_ids {
                self.jobs
                    .queue_facial_recognition(&face_id, false)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        ml_job::set_faces_recognized_at(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?;

        Ok(FaceDetectionOutcome::Success)
    }
}

#[derive(Debug)]
struct BoundingBox {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

fn parse_existing_faces(value: Option<serde_json::Value>) -> Vec<DetectFaceAssetFace> {
    value
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn iou(face: &DetectFaceAssetFace, new_box: &BoundingBox) -> f64 {
    let x1 = f64::max(face.bounding_box_x1 as f64, new_box.x1);
    let y1 = f64::max(face.bounding_box_y1 as f64, new_box.y1);
    let x2 = f64::min(face.bounding_box_x2 as f64, new_box.x2);
    let y2 = f64::min(face.bounding_box_y2 as f64, new_box.y2);

    let intersection = f64::max(0.0, x2 - x1) * f64::max(0.0, y2 - y1);
    let area1 = (face.bounding_box_x2 - face.bounding_box_x1) as f64
        * (face.bounding_box_y2 - face.bounding_box_y1) as f64;
    let area2 = (new_box.x2 - new_box.x1) * (new_box.y2 - new_box.y1);
    let union = area1 + area2 - intersection;

    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}
