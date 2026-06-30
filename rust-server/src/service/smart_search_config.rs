use serde_json::Value;
use sqlx::PgPool;

use crate::models::db::advisory_lock::{self, LOCK_CLIP_DIM_SIZE};
use crate::utils::system_config::is_smart_search_enabled;
use crate::utils::clip::get_clip_dim_size;
use crate::utils::vector::{delete_all_search_embeddings, get_dimension_size, set_dimension_size};

pub async fn sync_on_config_change(
    pool: &PgPool,
    new_config: &Value,
    old_config: Option<&Value>,
) -> Result<(), String> {
    let ml = new_config
        .get("machineLearning")
        .cloned()
        .unwrap_or_default();
    if !is_smart_search_enabled(&ml) {
        return Ok(());
    }

    match advisory_lock::run_with_try_lock(pool, LOCK_CLIP_DIM_SIZE, || async {
        sync_clip_dimensions(pool, new_config, old_config).await
    })
    .await
    .map_err(|err| err.to_string())?
    {
        Some(()) => Ok(()),
        None => {
            eprintln!("smart search config: could not acquire CLIP dimension lock, skipping sync");
            Ok(())
        }
    }
}

async fn sync_clip_dimensions(
    pool: &PgPool,
    new_config: &Value,
    old_config: Option<&Value>,
) -> Result<(), sqlx::Error> {
    let model_name = new_config
        .get("machineLearning")
        .and_then(|ml| ml.get("clip"))
        .and_then(|clip| clip.get("modelName"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let dim_size = get_clip_dim_size(model_name).map_err(|err| {
        sqlx::Error::Protocol(err)
    })?;
    let db_dim_size = get_dimension_size(pool, "smart_search", "embedding").await;

    let model_change = old_config.is_some_and(|old| {
        old.get("machineLearning")
            .and_then(|ml| ml.get("clip"))
            .and_then(|clip| clip.get("modelName"))
            .and_then(|value| value.as_str())
            != Some(model_name)
    });
    let dim_size_change = db_dim_size != dim_size;
    if !model_change && !dim_size_change {
        return Ok(());
    }

    if dim_size_change {
        println!(
            "Updating database CLIP dimension size from {db_dim_size} to {dim_size} for model {model_name}"
        );
        set_dimension_size(pool, dim_size, None)
            .await
            .map_err(|err| sqlx::Error::Protocol(err))?;
    } else {
        delete_all_search_embeddings(pool)
            .await
            .map_err(|err| sqlx::Error::Protocol(err))?;
    }

    Ok(())
}
