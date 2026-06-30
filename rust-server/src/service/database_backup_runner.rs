use std::path::{Path, PathBuf};
use std::process::Stdio;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use sqlx::PgPool;
use tokio::task::spawn_blocking;

use crate::models::db::system_metadata::get_json;
use crate::models::dto::env::EnvDto;
use crate::service::server::ServerService;
use crate::utils::database_backups::{
    is_failed_database_backup_name, is_legacy_pg_cluster_dump, is_valid_database_backup_name,
    is_valid_database_routine_backup_name,
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

    pub async fn restore_database_backup(
        &self,
        filename: &str,
        mut progress_cb: impl FnMut(&str, i32) + Send + 'static,
    ) -> Result<(), BackupRunnerError> {
        if !is_valid_database_backup_name(filename) {
            return Err(BackupRunnerError::Process(
                "Invalid backup file format!".into(),
            ));
        }

        let backup_path = self.storage.backups_folder().join(filename);
        if tokio::fs::metadata(&backup_path).await.is_err() {
            return Err(BackupRunnerError::Io("Backup file not found".into()));
        }

        progress_cb("backup", 5);

        let restore_point = self.create_backup("restore-point-").await?;
        progress_cb("restore", 0);

        let username = self.env.db_username.clone();
        let is_pg_cluster_dump = is_legacy_pg_cluster_dump(filename);
        let progress = std::sync::Arc::new(std::sync::Mutex::new(progress_cb));
        let restore_progress = std::sync::Arc::clone(&progress);
        let result = self
            .restore_from_file(&backup_path, &username, is_pg_cluster_dump, move |value| {
                if let Ok(mut cb) = restore_progress.lock() {
                    cb("restore", (value * 100.0) as i32);
                }
            })
            .await;

        if let Err(err) = result {
            eprintln!("database restore failed, rolling back: {err}");
            if let Ok(mut cb) = progress.lock() {
                cb("rollback", 0);
            }
            let rollback_progress = std::sync::Arc::clone(&progress);
            self.restore_from_file(
                std::path::Path::new(&restore_point),
                &username,
                false,
                move |value| {
                    if let Ok(mut cb) = rollback_progress.lock() {
                        cb("rollback", (value * 100.0) as i32);
                    }
                },
            )
            .await?;
            return Err(err);
        }

        if let Ok(mut cb) = progress.lock() {
            cb("migrations", 90);
        }

        crate::service::database_migrations::run(&self.env)
            .await
            .map_err(|err| BackupRunnerError::Process(err.to_string()))?;

        let has_admin = crate::models::db::users::UserDb::get_admin(&self.pool)
            .await
            .map_err(|err| BackupRunnerError::Sql(err.to_string()))?
            .is_some();
        if !has_admin {
            return Err(BackupRunnerError::Process(
                "Server health check failed, no admin exists.".into(),
            ));
        }

        crate::service::database_migrations::verify_schema(&self.pool)
            .await
            .map_err(|err| BackupRunnerError::Process(err.to_string()))?;

        if let Ok(mut cb) = progress.lock() {
            cb("restore", 100);
        }
        Ok(())
    }

    async fn restore_from_file(
        &self,
        backup_path: &Path,
        username: &str,
        is_pg_cluster_dump: bool,
        mut progress_cb: impl FnMut(f64) + Send + 'static,
    ) -> Result<(), BackupRunnerError> {
        let pg_version = self.postgres_version().await?;
        let major = parse_postgres_major(&pg_version).ok_or_else(|| {
            BackupRunnerError::UnsupportedPostgres(pg_version.clone())
        })?;
        let psql = resolve_pg_binary("psql", major);
        let args = self.psql_args(!is_pg_cluster_dump);
        let password = self.env.db_password.clone();
        let preamble = restore_preamble(username, is_pg_cluster_dump);

        let file_size = tokio::fs::metadata(backup_path)
            .await
            .map_err(|err| BackupRunnerError::Io(err.to_string()))?
            .len()
            .max(1);

        let backup_path = backup_path.to_path_buf();
        let psql_bin = psql.clone();
        let psql_args = args.clone();

        spawn_blocking(move || {
            run_psql_restore(
                &psql_bin,
                &psql_args,
                &password,
                &preamble,
                &backup_path,
                file_size,
                &mut progress_cb,
            )
        })
        .await
        .map_err(|err| BackupRunnerError::Process(err.to_string()))?
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

    fn psql_args(&self, single_transaction: bool) -> Vec<String> {
        let mut args = vec![
            "--username".into(),
            self.env.db_username.clone(),
            "--host".into(),
            self.env.db_url.clone(),
            "--port".into(),
            self.env.db_port.to_string(),
            "--dbname".into(),
            self.env.db_database_name.clone(),
        ];
        if single_transaction {
            args.push("--single-transaction".into());
        }
        args
    }
}

fn restore_preamble(username: &str, is_pg_cluster_dump: bool) -> String {
    let drop_connections = r#"
SELECT pg_terminate_backend(pid)
FROM pg_stat_activity
WHERE datname = current_database()
  AND pid <> pg_backend_pid();
"#;
    if is_pg_cluster_dump {
        format!("{drop_connections}\n\\c postgres\n")
    } else {
        format!(
            "{drop_connections}
DROP SCHEMA public CASCADE;
CREATE SCHEMA public;
GRANT ALL ON SCHEMA public TO \"{username}\";
GRANT ALL ON SCHEMA public TO public;
"
        )
    }
}

fn run_psql_restore<F>(
    psql: &Path,
    args: &[String],
    password: &str,
    preamble: &str,
    backup_path: &Path,
    file_size: u64,
    progress_cb: &mut F,
) -> Result<(), BackupRunnerError>
where
    F: FnMut(f64),
{
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};

    let mut child = Command::new(psql)
        .args(args)
        .env("PGPASSWORD", password)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| BackupRunnerError::Process(format!("failed to spawn psql: {err}")))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| BackupRunnerError::Process("psql stdin unavailable".into()))?;

    stdin
        .write_all(preamble.as_bytes())
        .map_err(|err| BackupRunnerError::Io(err.to_string()))?;

    let file = std::fs::File::open(backup_path)
        .map_err(|err| BackupRunnerError::Io(err.to_string()))?;
    let mut reader: Box<dyn Read> = if backup_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
        || backup_path
            .to_string_lossy()
            .ends_with(".sql.gz")
    {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };

    let mut buffer = [0u8; 64 * 1024];
    let mut bytes_read = 0u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| BackupRunnerError::Io(err.to_string()))?;
        if read == 0 {
            break;
        }
        stdin
            .write_all(&buffer[..read])
            .map_err(|err| BackupRunnerError::Io(err.to_string()))?;
        bytes_read += read as u64;
        progress_cb((bytes_read as f64 / file_size as f64).min(1.0));
    }

    drop(stdin);
    let output = child
        .wait()
        .map_err(|err| BackupRunnerError::Process(err.to_string()))?;
    if !output.success() {
        return Err(BackupRunnerError::Process(format!(
            "psql exited with status {output}"
        )));
    }

    Ok(())
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
