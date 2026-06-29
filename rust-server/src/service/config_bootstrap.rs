use std::sync::OnceLock;

use sqlx::PgPool;

use crate::models::dto::env::EnvDto;
use crate::service::ml_health;
use crate::utils::system_config::get_merged;
use crate::utils::workers::should_run_microservices_workers;

static RUNTIME_ENV: OnceLock<EnvDto> = OnceLock::new();

pub fn set_runtime_env(env: EnvDto) {
    let _ = RUNTIME_ENV.set(env);
}

pub async fn run(pool: &PgPool, env: &EnvDto) {
    set_runtime_env(env.clone());
    let config = match get_merged(pool).await {
        Ok(value) => value,
        Err(err) => {
            eprintln!("config bootstrap: failed to load config: {err}");
            return;
        }
    };

    let env_level = env
        .immich_log_level
        .as_ref()
        .map(|level| format!("{level:?}").to_lowercase());
    let level = ml_health::log_level_from_config(env_level.as_deref(), &config);
    println!(
        "config bootstrap: LogLevel={level}{}",
        if env.immich_log_level.is_some() {
            " (set via IMMICH_LOG_LEVEL)"
        } else {
            " (set via system config)"
        }
    );

    ml_health::setup(pool).await;

    if should_run_microservices_workers(env) {
        crate::utils::worker_concurrency::log_concurrency_settings(&config);
        crate::service::library_watcher::reload_watch_config(pool).await;
    }

    println!("config bootstrap: completed");
}

pub async fn on_config_update(pool: &PgPool) {
    let config = match get_merged(pool).await {
        Ok(value) => value,
        Err(err) => {
            eprintln!("config bootstrap: failed to reload config: {err}");
            return;
        }
    };

    ml_health::setup(pool).await;
    if RUNTIME_ENV
        .get()
        .is_some_and(|env| should_run_microservices_workers(env))
    {
        crate::service::library_watcher::reload_watch_config(pool).await;
        crate::service::workers::restart_all(&config).await;
    }
}
