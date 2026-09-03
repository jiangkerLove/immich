use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::workflow;
use crate::models::response::response::ErrorResp;
use crate::service::job::JobService;

pub async fn on_asset_trigger(
    pool: &PgPool,
    jobs: &JobService,
    user_id: &Uuid,
    asset_id: &Uuid,
    trigger: &str,
) -> Result<(), ErrorResp> {
    let workflows = workflow::search(pool, user_id, None, Some(trigger), Some(true), None)
        .await
        .map_err(|err| ErrorResp::ServerError(err.to_string()))?;

    for workflow in workflows {
        jobs.queue_workflow_asset_trigger(&workflow.id, asset_id, trigger)
            .await?;
    }

    Ok(())
}
