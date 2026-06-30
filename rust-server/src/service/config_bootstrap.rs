use std::sync::OnceLock;

use sqlx::PgPool;

use crate::models::dto::env::EnvDto;
use crate::service::job::JobService;
use crate::service::ml_health;
use crate::utils::system_config::get_merged;
use crate::utils::workers::should_run_microservices_workers;

static RUNTIME_ENV: OnceLock<EnvDto> = OnceLock::new();
static RUNTIME_JOBS: OnceLock<JobService> = OnceLock::new();

pub fn set_runtime_env(env: EnvDto) {
    let _ = RUNTIME_ENV.set(env);
}

fn set_runtime_jobs(jobs: &JobService) {
    let _ = RUNTIME_JOBS.set(jobs.clone());
}

pub async fn run(pool: &PgPool, env: &EnvDto, jobs: &JobService) {
    set_runtime_env(env.clone());
    set_runtime_jobs(jobs);
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

    if let Err(err) =
        crate::service::smart_search_config::sync_on_config_change(pool, &config, None).await
    {
        eprintln!("config bootstrap: smart search config sync failed: {err}");
    }

    if should_run_microservices_workers(env) {
        if let Err(err) = crate::service::geodata_import::init(pool, env, jobs).await {
            eprintln!("config bootstrap: geodata import failed: {err}");
        }
        if let Err(err) = crate::service::version_bootstrap::run(pool, jobs).await {
            eprintln!("config bootstrap: version bootstrap failed: {err}");
        }
        crate::utils::worker_concurrency::log_concurrency_settings(&config);
        crate::service::library_watcher::reload_watch_config(pool).await;
    }

    println!("config bootstrap: completed");
}

pub async fn on_config_update(pool: &PgPool, old_config: Option<&serde_json::Value>) {
    let config = match get_merged(pool).await {
        Ok(value) => value,
        Err(err) => {
            eprintln!("config bootstrap: failed to reload config: {err}");
            return;
        }
    };

    if let Err(err) =
        crate::service::smart_search_config::sync_on_config_change(pool, &config, old_config).await
    {
        eprintln!("config bootstrap: smart search config sync failed: {err}");
    }

    ml_health::setup(pool).await;
    if RUNTIME_ENV
        .get()
        .is_some_and(|env| should_run_microservices_workers(env))
    {
        if let (Some(old), Some(jobs)) = (old_config, RUNTIME_JOBS.get()) {
            let was_disabled =
                !crate::utils::system_config::json_bool(old, &["newVersionCheck", "enabled"], false);
            let now_enabled =
                crate::utils::system_config::json_bool(&config, &["newVersionCheck", "enabled"], false);
            if was_disabled && now_enabled {
                if let Err(err) = jobs.queue_version_check().await {
                    eprintln!("config bootstrap: failed to queue VersionCheck: {err}");
                }
            }
        }

        crate::service::library_watcher::reload_watch_config(pool).await;
        crate::service::workers::restart_all(&config).await;
        crate::service::scheduler_notify::wake_all();
    }
}
