use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::ml_job;
use crate::models::db::person;
use crate::models::db::system_metadata::{
    get_facial_recognition_state, get_machine_learning_config, is_facial_recognition_enabled,
    set_facial_recognition_state, FacialRecognitionState,
};
use crate::service::job::JobService;
use crate::utils::workers::{QUEUE_FACE, QUEUE_FACIAL, QUEUE_THUMBNAIL};

const JOBS_BATCH_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacialRecognitionOutcome {
    Skipped,
    Failed,
    Success,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacialRecognitionQueueAllOutcome {
    Skipped,
    Success,
}

#[derive(Clone)]
pub struct FacialRecognitionService {
    pool: PgPool,
    jobs: JobService,
}

impl FacialRecognitionService {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self { pool, jobs }
    }

    pub async fn queue_all(
        &self,
        force: bool,
        nightly: bool,
        cluster_group_id: Option<Uuid>,
    ) -> Result<FacialRecognitionQueueAllOutcome, String> {
        let config = get_machine_learning_config(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if !is_facial_recognition_enabled(&config) {
            return Ok(FacialRecognitionQueueAllOutcome::Skipped);
        }
        if !crate::utils::vector::face_search_available(&self.pool).await {
            return Ok(FacialRecognitionQueueAllOutcome::Skipped);
        }

        self.jobs
            .wait_for_queue_completion(&[QUEUE_THUMBNAIL, QUEUE_FACE])
            .await
            .map_err(|err| err.to_string())?;

        if nightly {
            let state = get_facial_recognition_state(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
            let latest_face_date = ml_job::get_latest_face_date(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
            if let (Some(last_run), Some(latest_date)) = (&state.last_run, &latest_face_date) {
                if last_run > latest_date {
                    return Ok(FacialRecognitionQueueAllOutcome::Skipped);
                }
            }
        }

        if !force {
            let waiting = self
                .jobs
                .get_queue_waiting_count(QUEUE_FACIAL)
                .await
                .map_err(|err| err.to_string())?;
            if waiting > 0 {
                return Ok(FacialRecognitionQueueAllOutcome::Skipped);
            }
        }

        if force {
            if let Some(cluster_group_id) = cluster_group_id.as_ref() {
                person::unassign_ml_faces_for_cluster(&self.pool, cluster_group_id)
                    .await
                    .map_err(|err| err.to_string())?;
            } else {
                person::unassign_ml_faces(&self.pool)
                    .await
                    .map_err(|err| err.to_string())?;
            }
            self.jobs
                .queue_person_cleanup()
                .await
                .map_err(|err| err.to_string())?;
            person::vacuum_faces(&self.pool, false)
                .await
                .map_err(|err| err.to_string())?;
        }

        ml_job::prewarm_face_vectors(&self.pool).await;

        let face_ids = ml_job::stream_unassigned_ml_faces(
            &self.pool,
            force,
            cluster_group_id.as_ref(),
        )
            .await
            .map_err(|err| err.to_string())?;

        for chunk in face_ids.chunks(JOBS_BATCH_SIZE) {
            for face_id in chunk {
                self.jobs
                    .queue_facial_recognition(face_id, false)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        let last_run = chrono::Utc::now().to_rfc3339();
        set_facial_recognition_state(
            &self.pool,
            &FacialRecognitionState {
                last_run: Some(last_run),
            },
        )
        .await
        .map_err(|err| err.to_string())?;

        Ok(FacialRecognitionQueueAllOutcome::Success)
    }

    pub async fn recognize_face(
        &self,
        face_id: &Uuid,
        deferred: bool,
    ) -> Result<FacialRecognitionOutcome, String> {
        let config = get_machine_learning_config(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        if !is_facial_recognition_enabled(&config) {
            return Ok(FacialRecognitionOutcome::Skipped);
        }
        if !crate::utils::vector::face_search_available(&self.pool).await {
            return Ok(FacialRecognitionOutcome::Skipped);
        }

        let Some(face) = ml_job::get_for_facial_recognition(&self.pool, face_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(FacialRecognitionOutcome::Failed);
        };

        if face.source_type != "machine-learning" {
            return Ok(FacialRecognitionOutcome::Skipped);
        }

        let Some(embedding) = face.embedding else {
            return Ok(FacialRecognitionOutcome::Failed);
        };

        if face.person_id.is_some() {
            return Ok(FacialRecognitionOutcome::Skipped);
        }

        let min_birth_date = Some(face.file_created_at);
        let matches = ml_job::search_faces(
            &self.pool,
            &embedding,
            &[face.owner_id],
            config.facial_recognition.max_distance,
            config.facial_recognition.min_faces as i64,
            false,
            min_birth_date,
        )
        .await
        .map_err(|err| err.to_string())?;

        if config.facial_recognition.min_faces > 1 && matches.len() <= 1 {
            return Ok(FacialRecognitionOutcome::Skipped);
        }

        let is_core = matches.len() >= config.facial_recognition.min_faces as usize
            && face.visibility == "timeline";
        if !is_core && !deferred {
            self.jobs
                .queue_facial_recognition(face_id, true)
                .await
                .map_err(|err| err.to_string())?;
            return Ok(FacialRecognitionOutcome::Skipped);
        }

        let mut person_id = matches.iter().find_map(|m| m.person_id);

        if person_id.is_none() {
            let with_person = ml_job::search_faces(
                &self.pool,
                &embedding,
                &[face.owner_id],
                config.facial_recognition.max_distance,
                1,
                true,
                min_birth_date,
            )
            .await
            .map_err(|err| err.to_string())?;
            person_id = with_person.first().and_then(|m| m.person_id);
        }

        if is_core && person_id.is_none() {
            let new_person =
                person::create_for_detected_face(&self.pool, &face.owner_id, face_id)
                    .await
                    .map_err(|err| err.to_string())?;
            self.jobs
                .queue_person_generate_thumbnail(&new_person.id)
                .await
                .map_err(|err| err.to_string())?;
            person_id = Some(new_person.id);
        }

        if let Some(person_id) = person_id {
            person::reassign_face(&self.pool, face_id, &person_id)
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(FacialRecognitionOutcome::Success)
    }
}
