use std::collections::HashMap;
use std::time::Duration;

use bullmq_rs::JobState;
use serde::Serialize;
use serde_json::Value;

use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::job::JobService;
use crate::utils::permission::require_admin;

pub const ALL_QUEUES: &[&str] = &[
    "thumbnailGeneration",
    "metadataExtraction",
    "videoConversion",
    "faceDetection",
    "facialRecognition",
    "smartSearch",
    "duplicateDetection",
    "backgroundTask",
    "storageTemplateMigration",
    "migration",
    "search",
    "sidecar",
    "library",
    "notifications",
    "backupDatabase",
    "ocr",
    "workflow",
    "integrityCheck",
    "editor",
];

#[derive(Clone)]
pub struct QueueService {
    jobs: JobService,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueStatisticsResponse {
    pub active: i64,
    pub completed: i64,
    pub failed: i64,
    pub delayed: i64,
    pub waiting: i64,
    pub paused: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueResponse {
    pub name: String,
    pub is_paused: bool,
    pub statistics: QueueStatisticsResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueStatusLegacyResponse {
    pub is_active: bool,
    pub is_paused: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueLegacyResponse {
    pub queue_status: QueueStatusLegacyResponse,
    pub job_counts: QueueStatisticsResponse,
}

#[derive(Debug, Serialize)]
pub struct QueuesLegacyResponse {
    #[serde(flatten)]
    pub queues: HashMap<String, QueueLegacyResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueJobResponse {
    pub id: Option<String>,
    pub name: String,
    pub data: Value,
    pub timestamp: i64,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueUpdateReq {
    pub is_paused: Option<bool>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueDeleteReq {
    pub failed: Option<bool>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueJobSearchQuery {
    pub status: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueCommandReq {
    pub command: String,
    pub force: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ManualJobCreateReq {
    pub name: String,
}

impl QueueService {
    pub fn new(jobs: JobService) -> Self {
        Self { jobs }
    }

    pub async fn get_all(&self, auth: &AuthDto) -> Result<Vec<QueueResponse>, ErrorResp> {
        require_admin(auth)?;
        let mut responses = Vec::with_capacity(ALL_QUEUES.len());
        for name in ALL_QUEUES {
            responses.push(self.get_by_name(name).await?);
        }
        Ok(responses)
    }

    pub async fn get_all_legacy(&self, auth: &AuthDto) -> Result<QueuesLegacyResponse, ErrorResp> {
        let responses = self.get_all(auth).await?;
        let mut queues = HashMap::new();
        for response in responses {
            queues.insert(response.name.clone(), map_queue_legacy(&response));
        }
        Ok(QueuesLegacyResponse { queues })
    }

    pub async fn get(&self, auth: &AuthDto, name: &str) -> Result<QueueResponse, ErrorResp> {
        require_admin(auth)?;
        validate_queue_name(name)?;
        self.get_by_name(name).await
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        name: &str,
        dto: &QueueUpdateReq,
    ) -> Result<QueueResponse, ErrorResp> {
        require_admin(auth)?;
        validate_queue_name(name)?;

        let queue = self.jobs.json_queue(name).await?;

        if dto.is_paused == Some(true) {
            if name == "backgroundTask" {
                return Err(ErrorResp::BadRequest(
                    "The BackgroundTask queue cannot be paused".to_string(),
                ));
            }
            queue.pause().await.map_err(queue_err)?;
        }

        if dto.is_paused == Some(false) {
            queue.resume().await.map_err(queue_err)?;
        }

        self.get_by_name(name).await
    }

    pub async fn search_jobs(
        &self,
        auth: &AuthDto,
        name: &str,
        query: &QueueJobSearchQuery,
    ) -> Result<Vec<QueueJobResponse>, ErrorResp> {
        require_admin(auth)?;
        validate_queue_name(name)?;

        let queue = self.jobs.json_queue(name).await?;
        let states = parse_job_states(query.status.as_deref());
        let jobs = queue.get_jobs(&states, 0, 1000, true).await.map_err(queue_err)?;

        Ok(jobs
            .into_iter()
            .map(|job| QueueJobResponse {
                id: Some(job.id),
                name: job.name,
                data: job.data,
                timestamp: job.timestamp as i64,
            })
            .collect())
    }

    pub async fn empty_queue(
        &self,
        auth: &AuthDto,
        name: &str,
        dto: &QueueDeleteReq,
    ) -> Result<(), ErrorResp> {
        require_admin(auth)?;
        validate_queue_name(name)?;

        let queue = self.jobs.json_queue(name).await?;
        queue.drain().await.map_err(queue_err)?;

        if dto.failed.unwrap_or(false) {
            queue
                .clean(Duration::from_secs(0), 1000, JobState::Failed)
                .await
                .map_err(queue_err)?;
        }

        Ok(())
    }

    pub async fn run_legacy_command(
        &self,
        auth: &AuthDto,
        name: &str,
        dto: &QueueCommandReq,
    ) -> Result<QueueLegacyResponse, ErrorResp> {
        require_admin(auth)?;
        validate_queue_name(name)?;

        match dto.command.as_str() {
            "start" => {
                self.jobs
                    .queue_start_command(name, dto.force.unwrap_or(false))
                    .await?;
            }
            "pause" => {
                self.jobs.json_queue(name).await?.pause().await.map_err(queue_err)?;
            }
            "resume" => {
                self.jobs.json_queue(name).await?.resume().await.map_err(queue_err)?;
            }
            "empty" => {
                self.jobs.json_queue(name).await?.drain().await.map_err(queue_err)?;
            }
            "clear-failed" => {
                self.jobs
                    .json_queue(name)
                    .await?
                    .clean(Duration::from_secs(0), 1000, JobState::Failed)
                    .await
                    .map_err(queue_err)?;
            }
            _ => {
                return Err(ErrorResp::BadRequest(format!(
                    "Invalid queue command: {}",
                    dto.command
                )));
            }
        }

        Ok(map_queue_legacy(&self.get_by_name(name).await?))
    }

    pub async fn create_manual_job(
        &self,
        auth: &AuthDto,
        dto: &ManualJobCreateReq,
    ) -> Result<(), ErrorResp> {
        require_admin(auth)?;
        self.jobs.create_manual_job(&dto.name).await
    }

    async fn get_by_name(&self, name: &str) -> Result<QueueResponse, ErrorResp> {
        let queue = self.jobs.json_queue(name).await?;
        let counts = queue.get_job_counts().await.map_err(queue_err)?;
        let is_paused = queue.is_paused().await.map_err(queue_err)?;

        Ok(QueueResponse {
            name: name.to_string(),
            is_paused,
            statistics: map_statistics(&counts),
        })
    }
}

fn map_statistics(counts: &HashMap<JobState, u64>) -> QueueStatisticsResponse {
    QueueStatisticsResponse {
        active: counts.get(&JobState::Active).copied().unwrap_or(0) as i64,
        completed: counts.get(&JobState::Completed).copied().unwrap_or(0) as i64,
        failed: counts.get(&JobState::Failed).copied().unwrap_or(0) as i64,
        delayed: counts.get(&JobState::Delayed).copied().unwrap_or(0) as i64,
        waiting: counts.get(&JobState::Wait).copied().unwrap_or(0) as i64,
        paused: counts.get(&JobState::Paused).copied().unwrap_or(0) as i64,
    }
}

fn map_queue_legacy(response: &QueueResponse) -> QueueLegacyResponse {
    QueueLegacyResponse {
        queue_status: QueueStatusLegacyResponse {
            is_paused: response.is_paused,
            is_active: response.statistics.active > 0,
        },
        job_counts: response.statistics.clone(),
    }
}

fn validate_queue_name(name: &str) -> Result<(), ErrorResp> {
    if ALL_QUEUES.contains(&name) {
        Ok(())
    } else {
        Err(ErrorResp::BadRequest(format!("Invalid queue name: {name}")))
    }
}

fn parse_job_states(status: Option<&[String]>) -> Vec<JobState> {
    let Some(status) = status else {
        return vec![
            JobState::Active,
            JobState::Failed,
            JobState::Completed,
            JobState::Delayed,
            JobState::Wait,
            JobState::Paused,
        ];
    };

    status
        .iter()
        .filter_map(|value| match value.as_str() {
            "active" => Some(JobState::Active),
            "failed" => Some(JobState::Failed),
            "completed" => Some(JobState::Completed),
            "delayed" => Some(JobState::Delayed),
            "waiting" => Some(JobState::Wait),
            "paused" => Some(JobState::Paused),
            _ => None,
        })
        .collect()
}

fn queue_err(err: bullmq_rs::BullmqError) -> ErrorResp {
    ErrorResp::ServerError(err.to_string())
}
