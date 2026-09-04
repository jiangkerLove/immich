use std::sync::Arc;

use bullmq_rs::{RedisConnection, WorkerBuilder};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::dto::env::EnvDto;
use crate::service::job::JobService;
use crate::service::plugin_runtime::PluginRuntime;
use crate::service::websocket::WebSocketHub;
use crate::service::workflow_execution::{WorkflowExecutionOutcome, WorkflowExecutionService};

const BULL_PREFIX: &str = "immich_bull";
const QUEUE_WORKFLOW: &str = "workflow";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowAssetTriggerJob {
    workflow_id: Uuid,
    asset_id: Uuid,
    #[allow(dead_code)]
    trigger: Option<String>,
}

#[derive(Clone)]
pub struct WorkflowProcessor {
    service: Arc<WorkflowExecutionService>,
}

impl WorkflowProcessor {
    pub fn new(
        pool: PgPool,
        runtime: Arc<PluginRuntime>,
        jobs: JobService,
        websocket: WebSocketHub,
    ) -> Self {
        Self {
            service: Arc::new(WorkflowExecutionService::new(
                pool, runtime, jobs, websocket,
            )),
        }
    }

    async fn process(&self, name: &str, data: &Value) -> Result<JobWorkerStatus, String> {
        match name {
            "WorkflowAssetTrigger" => {
                let job: WorkflowAssetTriggerJob =
                    serde_json::from_value(data.clone()).map_err(|err| err.to_string())?;
                match self
                    .service
                    .execute(&job.workflow_id, &job.asset_id)
                    .await?
                {
                    WorkflowExecutionOutcome::Success => Ok(JobWorkerStatus::Success),
                    WorkflowExecutionOutcome::Skipped => Ok(JobWorkerStatus::Skipped),
                    WorkflowExecutionOutcome::Failed => Ok(JobWorkerStatus::Failed),
                }
            }
            other => {
                tracing::warn!("workflow job {other} is not implemented in rust-server yet; skipping");
                Ok(JobWorkerStatus::Skipped)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobWorkerStatus {
    Success,
    Skipped,
    Failed,
}

impl JobWorkerStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

pub fn spawn(
    pool: PgPool,
    redis_url: String,
    _env: EnvDto,
    jobs: JobService,
    websocket: WebSocketHub,
    concurrency: usize,
) {
    tokio::spawn(async move {
        let runtime = Arc::new(PluginRuntime::new(
            pool.clone(),
            jobs.clone(),
            websocket.clone(),
        ));
        let processor = Arc::new(WorkflowProcessor::new(
            pool.clone(),
            runtime.clone(),
            jobs.clone(),
            websocket.clone(),
        ));
        let loader = WorkflowExecutionService::new(pool, runtime, jobs, websocket);

        if let Err(err) = loader.load_plugins().await {
            tracing::error!("workflow plugin load failed: {err}");
        }

        let worker = WorkerBuilder::new(QUEUE_WORKFLOW)
            .prefix(BULL_PREFIX)
            .connection(RedisConnection::new(redis_url))
            .concurrency(concurrency)
            .build::<Value>();

        let handle = worker
            .start(move |job| {
                let processor = processor.clone();
                async move {
                    let job_name = job.name.clone();
                    crate::service::workers::wrap_status_job(QUEUE_WORKFLOW, &job_name, || async {
                        processor
                            .process(&job_name, &job.data)
                            .await
                            .map(|status| status.as_str())
                    })
                    .await
                }
            })
            .await;

        match handle {
            Ok(worker_handle) => {
                crate::service::worker_registry::register(worker_handle);
                std::future::pending::<()>().await;
            }
            Err(err) => {
                tracing::error!("workflow worker failed to start: {err}");
            }
        }
    });
}
