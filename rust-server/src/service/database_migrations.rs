use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;

use crate::models::dto::env::EnvDto;

const MIGRATION_SCRIPT: &str = "bin/run-kysely-migrations.cjs";

#[derive(Debug)]
pub enum MigrationError {
    NotConfigured(String),
    Io(String),
    Failed(String),
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured(message) => write!(f, "{message}"),
            Self::Io(message) => write!(f, "io error: {message}"),
            Self::Failed(message) => write!(f, "migration failed: {message}"),
        }
    }
}

impl std::error::Error for MigrationError {}

pub async fn run(env: &EnvDto) -> Result<(), MigrationError> {
    if env.db_skip_migrations.unwrap_or(false) {
        println!("database migrations: skipped (DB_SKIP_MIGRATIONS=true)");
        return Ok(());
    }

    let server_home = resolve_server_home(env)?;
    let script = server_home.join(MIGRATION_SCRIPT);
    if !script.exists() {
        return Err(MigrationError::NotConfigured(format!(
            "migration script not found at {}",
            script.display()
        )));
    }

    let migrations_dir = server_home.join("dist/schema/migrations");
    if !migrations_dir.is_dir() {
        return Err(MigrationError::NotConfigured(format!(
            "compiled migrations not found at {} (build the Node server first)",
            migrations_dir.display()
        )));
    }

    println!("database migrations: running kysely migrations via {}", script.display());

    let output = Command::new("node")
        .arg(&script)
        .current_dir(&server_home)
        .env("DB_HOSTNAME", &env.db_url)
        .env("DB_PORT", env.db_port.to_string())
        .env("DB_USERNAME", &env.db_username)
        .env("DB_PASSWORD", &env.db_password)
        .env("DB_DATABASE_NAME", &env.db_database_name)
        .env(
            "IMMICH_ENV",
            match env.immich_env {
                Some(crate::models::dto::env::ImmichEnvironment::Development) => "development",
                Some(crate::models::dto::env::ImmichEnvironment::Test) => "test",
                _ => "production",
            },
        )
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .await
        .map_err(|err| MigrationError::Io(err.to_string()))?;

    if !output.status.success() {
        return Err(MigrationError::Failed(format!(
            "node exited with status {}",
            output.status
        )));
    }

    Ok(())
}

fn resolve_server_home(env: &EnvDto) -> Result<PathBuf, MigrationError> {
    if let Some(path) = env.immich_server_path.as_ref() {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Ok(path);
        }
    }

    for candidate in candidate_server_paths() {
        if candidate.join(MIGRATION_SCRIPT).exists() {
            return Ok(candidate);
        }
    }

    Err(MigrationError::NotConfigured(
        "could not locate Immich server directory for kysely migrations; set IMMICH_SERVER_PATH"
            .into(),
    ))
}

fn candidate_server_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(current) = std::env::current_dir() {
        paths.push(current.join("server"));
        if current.ends_with("rust-server") {
            paths.push(current.parent().unwrap_or(&current).join("server"));
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join("server"));
            paths.push(dir.join("../server"));
        }
    }

    paths.push(PathBuf::from("/usr/src/app/server"));
    paths
}

pub async fn verify_schema(pool: &sqlx::PgPool) -> Result<(), MigrationError> {
    let report = crate::models::db::schema_check::run(pool)
        .await
        .map_err(|err| MigrationError::Failed(err.to_string()))?;

    if crate::models::db::schema_check::print_report(&report) {
        return Err(MigrationError::Failed(
            "schema check failed after restore".into(),
        ));
    }

    Ok(())
}
