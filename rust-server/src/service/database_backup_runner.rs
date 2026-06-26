use std::path::{Path, PathBuf};
use std::process::Stdio;

use flate2::write::GzEncoder;
use flate2::Compression;
use sqlx::PgPool;
use tokio::task::spawn_blocking;

use crate::models::db::system_metadata::get_json;
use crate::models::dto::env::EnvDto;
use crate::service::server::ServerService;
use crate::utils::database_backups::{
    is_failed_database_backup_name, is_valid_database_routine_backup_name,
};
use crate::utils::storage::StoragePaths;

#[derive(Debug)]
pub enum BackupRunnerError {
    UnsupportedPostgres(String),
    Io(String),
    Process(String),
    Sql(String),
}

impl std::fmt::Display for BackupRunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPostgres(v) => write!(f, "unsupported PostgreSQL version: {v}"),
            Self::Io(v) => write!(f, "io error: {v}"),
            Self::Process(v) => write!(f, "process error: {v}"),
            Self::Sql(v) => write!(f, "sql error: {v}"),
        }
    }
}

impl std::error::Error for BackupRunnerError {}

#[derive(Clone)]
pub struct DatabaseBackupRunner {
    pool: PgPool,
    storage: StoragePaths,
    env: EnvDto,
}

impl DatabaseBackupRunner {
    pub fn new(pool: PgPool, storage: StoragePaths, env: EnvDto) -> Self {
        Self { pool, storage, env }
    }

    pub async fn run_backup(&self) -> Result<(), BackupRunnerError> {
        self.create_backup("").await?;
        self.cleanup_backups().await?;
        Ok(())
    }

    async fn create_backup(&self, filename_prefix: &str) -> Result<String, BackupRunnerError> {
        let pg_version = self.postgres_version().await?;
        let major = parse_postgres_major(&pg_version).ok_or_else(|| {
            BackupRunnerError::UnsupportedPostgres(pg_version.clone())
        })?;
        if !(14..=18).contains(&major) {
            return Err(BackupRunnerError::UnsupportedPostgres(pg_version));
        }

        let pg_dump = resolve_pg_binary("pg_dump", major);
        let args = self.pg_dump_args();
        let password = self.env.db_password.clone();

        let version = ServerService::version();
        let server_version = format!(
            "v{}.{}.{}",
            version.major, version.minor, version.patch
        );
        let timestamp = chrono::Local::now().format("%Y%m%dT%H%M%S");
        let pg_short = pg_version.split_whitespace().nth(1).unwrap_or("unknown");
        let filename = format!(
            "{filename_prefix}immich-db-backup-{timestamp}-v{server_version}-pg{pg_short}.sql.gz"
        );
        let backup_dir = self.storage.backups_folder();
        tokio::fs::create_dir_all(&backup_dir)
            .await
            .map_err(|err| BackupRunnerError::Io(err.to_string()))?;

        let backup_path = backup_dir.join(&filename);
        let temp_path = backup_dir.join(format!("{filename}.tmp"));

        let dump_bin = pg_dump.clone();
        let temp = temp_path.clone();
        let run_result = spawn_blocking(move || {
            run_pg_dump_gzip(&dump_bin, &args, &password, &temp)
        })
        .await
        .map_err(|err| BackupRunnerError::Process(err.to_string()))?;

        if let Err(err) = run_result {
            let _ = std::fs::remove_file(&temp_path);
            return Err(err);
        }

        tokio::fs::rename(&temp_path, &backup_path)
            .await
            .map_err(|err| BackupRunnerError::Io(err.to_string()))?;

        Ok(backup_path.to_string_lossy().into_owned())
    }

    async fn cleanup_backups(&self) -> Result<(), BackupRunnerError> {
        let keep_last = self.backup_keep_last_amount().await?;
        let backup_dir = self.storage.backups_folder();
        let mut entries = tokio::fs::read_dir(&backup_dir)
            .await
            .map_err(|err| BackupRunnerError::Io(err.to_string()))?;

        let mut routine = Vec::new();
        let mut failed = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|err| BackupRunnerError::Io(err.to_string()))?
        {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if is_valid_database_routine_backup_name(&name) {
                routine.push(name);
            } else if is_failed_database_backup_name(&name) {
                failed.push(name);
            }
        }

        routine.sort();
        routine.reverse();

        let mut to_delete = routine.into_iter().skip(keep_last).collect::<Vec<_>>();
        to_delete.append(&mut failed);

        for filename in to_delete {
            let path = backup_dir.join(filename);
            if tokio::fs::metadata(&path).await.is_ok() {
                tokio::fs::remove_file(path)
                    .await
                    .map_err(|err| BackupRunnerError::Io(err.to_string()))?;
            }
        }

        Ok(())
    }

    async fn postgres_version(&self) -> Result<String, BackupRunnerError> {
        sqlx::query_scalar("SELECT version()")
            .fetch_one(&self.pool)
            .await
            .map_err(|err| BackupRunnerError::Sql(err.to_string()))
    }

    async fn backup_keep_last_amount(&self) -> Result<usize, BackupRunnerError> {
        let config = get_json(&self.pool, "system-config")
            .await
            .map_err(|err| BackupRunnerError::Sql(err.to_string()))?;
        Ok(config
            .and_then(|value| {
                value
                    .get("backup")
                    .and_then(|backup| backup.get("database"))
                    .and_then(|database| database.get("keepLastAmount"))
                    .and_then(|amount| amount.as_u64())
            })
            .unwrap_or(14) as usize)
    }

    fn pg_dump_args(&self) -> Vec<String> {
        vec![
            "--username".into(),
            self.env.db_username.clone(),
            "--host".into(),
            self.env.db_url.clone(),
            "--port".into(),
            self.env.db_port.to_string(),
            "--clean".into(),
            "--if-exists".into(),
            self.env.db_database_name.clone(),
        ]
    }
}

fn parse_postgres_major(version: &str) -> Option<u32> {
    version
        .split_whitespace()
        .nth(1)
        .and_then(|part| part.split('.').next())
        .and_then(|major| major.parse().ok())
}

fn resolve_pg_binary(tool: &str, major: u32) -> PathBuf {
    let linux_path = PathBuf::from(format!("/usr/lib/postgresql/{major}/bin/{tool}"));
    if linux_path.exists() {
        return linux_path;
    }
    PathBuf::from(tool)
}

fn run_pg_dump_gzip(
    bin: &Path,
    args: &[String],
    password: &str,
    output_path: &Path,
) -> Result<(), BackupRunnerError> {
    let mut child = std::process::Command::new(bin)
        .args(args)
        .env("PGPASSWORD", password)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| BackupRunnerError::Process(format!("failed to spawn pg_dump: {err}")))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| BackupRunnerError::Process("pg_dump stdout unavailable".into()))?;

    let file = std::fs::File::create(output_path)
        .map_err(|err| BackupRunnerError::Io(err.to_string()))?;
    let mut encoder = GzEncoder::new(file, Compression::default());
    std::io::copy(&mut stdout, &mut encoder)
        .map_err(|err| BackupRunnerError::Io(err.to_string()))?;
    encoder
        .finish()
        .map_err(|err| BackupRunnerError::Io(err.to_string()))?;

    let status = child
        .wait()
        .map_err(|err| BackupRunnerError::Process(err.to_string()))?;
    if !status.success() {
        return Err(BackupRunnerError::Process(format!(
            "pg_dump exited with status {status}"
        )));
    }

    Ok(())
}
