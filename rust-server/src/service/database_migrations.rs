//! Database schema migrations via sqlx + upstream Kysely parity lock.
//!
//! ## Model
//! - `migrations/1_baseline.sql` = fused end-state of **all** Kysely names in
//!   `migrations/baseline_lock.json` (`fused_kysely_migrations`).
//! - After that baseline is locked in use, newer upstream Kysely become
//!   `migrations/2_*.sql`… Update the lock when you absorb them.
//!
//! ## Runtime (automatic)
//! 1. Bridge existing Immich schemas into `_sqlx_migrations` v1 if needed.
//! 2. Apply pending sqlx migrations (`migrate!`).
//! 3. Print status; warn if DB/`server` Kysely names are ahead of the lock.
//!
//! No Node / `IMMICH_SERVER_PATH` required for migrations.

use sqlx::PgPool;
use sqlx::migrate::{Migrate, Migration, Migrator};

use crate::models::dto::env::EnvDto;

include!(concat!(env!("OUT_DIR"), "/baseline_lock.rs"));
include!(concat!(env!("OUT_DIR"), "/kysely_migrations.rs"));

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

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

/// Auto-check + initialize schema (call on every API boot / admin migrate).
pub async fn run(pool: &PgPool, env: &EnvDto) -> Result<(), MigrationError> {
    if env.db_skip_migrations.unwrap_or(false) {
        println!("database migrations: skipped (DB_SKIP_MIGRATIONS=true)");
        return Ok(());
    }

    println!(
        "database migrations: sqlx baseline v{} ({}); lock fused {} Kysely name(s)",
        SQLX_BASELINE_VERSION,
        SQLX_BASELINE_FILE,
        FUSED_KYSELY_MIGRATIONS.len()
    );

    bridge_legacy_schema_if_needed(pool).await?;
    bridge_incremental_if_kysely_already_applied(pool).await?;

    println!(
        "database migrations: applying pending sqlx ({} file(s) in crate)",
        MIGRATOR.iter().count()
    );

    MIGRATOR
        .run(pool)
        .await
        .map_err(|err| MigrationError::Failed(err.to_string()))?;

    ensure_core_schema(pool).await?;
    let status = collect_status(pool).await?;
    print_status(&status);
    warn_kysely_ahead_of_lock(&status);

    Ok(())
}

#[derive(Debug, Default)]
pub struct MigrationStatus {
    pub sqlx_applied: Vec<(i64, String)>,
    pub sqlx_pending: Vec<(i64, String)>,
    pub asset_table_present: bool,
    pub kysely_table_present: bool,
    pub kysely_applied: Vec<String>,
    pub kysely_ahead_of_lock: Vec<String>,
    pub lock_fused_count: usize,
    pub lock_baseline_version: i64,
}

pub async fn status(pool: &PgPool) -> Result<MigrationStatus, MigrationError> {
    collect_status(pool).await
}

pub fn print_status(status: &MigrationStatus) {
    println!(
        "database migrations: status — asset_table={} sqlx_applied={} sqlx_pending={} \
         kysely_rows={} kysely_ahead_of_lock={} (lock baseline v{}, fused {})",
        status.asset_table_present,
        status.sqlx_applied.len(),
        status.sqlx_pending.len(),
        status.kysely_applied.len(),
        status.kysely_ahead_of_lock.len(),
        status.lock_baseline_version,
        status.lock_fused_count,
    );
    if !status.sqlx_applied.is_empty() {
        let latest = status.sqlx_applied.last().unwrap();
        println!(
            "database migrations: latest sqlx version={} ({})",
            latest.0, latest.1
        );
    }
    if !status.kysely_ahead_of_lock.is_empty() {
        println!(
            "database migrations: WARNING DB/server has Kysely migration(s) not in baseline_lock — \
             absorb into sqlx (new N_*.sql or merged file) then update migrations/baseline_lock.json: {}",
            status.kysely_ahead_of_lock.join(", ")
        );
    }
}

fn warn_kysely_ahead_of_lock(status: &MigrationStatus) {
    if status.kysely_ahead_of_lock.is_empty() {
        return;
    }
    eprintln!(
        "database migrations: schema may be ahead of this rust-server sqlx lock; \
         sync upstream Kysely changes into migrations/ before relying on new columns"
    );
}

async fn ensure_core_schema(pool: &PgPool) -> Result<(), MigrationError> {
    if table_exists(pool, "asset").await? {
        return Ok(());
    }
    Err(MigrationError::Failed(
        "core table \"asset\" missing after sqlx migrate — baseline did not initialize".into(),
    ))
}

async fn collect_status(pool: &PgPool) -> Result<MigrationStatus, MigrationError> {
    let asset_table_present = table_exists(pool, "asset").await?;
    let kysely_table_present = table_exists(pool, "kysely_migrations").await?;
    let sqlx_table_present = table_exists(pool, "_sqlx_migrations").await?;

    let sqlx_applied = if sqlx_table_present {
        sqlx::query_as::<_, (i64, String)>(
            r#"
                SELECT version, description
                FROM _sqlx_migrations
                WHERE success = true
                ORDER BY version
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|err| MigrationError::Failed(err.to_string()))?
    } else {
        Vec::new()
    };

    let applied_versions: std::collections::HashSet<i64> =
        sqlx_applied.iter().map(|(v, _)| *v).collect();
    let sqlx_pending: Vec<(i64, String)> = MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .filter(|m| !applied_versions.contains(&m.version))
        .map(|m| (m.version, m.description.to_string()))
        .collect();

    let kysely_applied = if kysely_table_present {
        sqlx::query_scalar::<_, String>(r#"SELECT name FROM kysely_migrations ORDER BY name"#)
            .fetch_all(pool)
            .await
            .map_err(|err| MigrationError::Failed(err.to_string()))?
    } else {
        Vec::new()
    };

    let fused: std::collections::HashSet<&str> = FUSED_KYSELY_MIGRATIONS.iter().copied().collect();
    let mut kysely_ahead_of_lock: Vec<String> = kysely_applied
        .iter()
        .filter(|name| !fused.contains(name.as_str()))
        .cloned()
        .collect();

    // Also surface compile-time upstream names ahead of lock (from build.rs).
    for name in KYSELY_MIGRATION_NAMES {
        if !fused.contains(*name) && !kysely_ahead_of_lock.iter().any(|n| n == name) {
            kysely_ahead_of_lock.push((*name).to_string());
        }
    }
    kysely_ahead_of_lock.sort();
    kysely_ahead_of_lock.dedup();

    Ok(MigrationStatus {
        sqlx_applied,
        sqlx_pending,
        asset_table_present,
        kysely_table_present,
        kysely_applied,
        kysely_ahead_of_lock,
        lock_fused_count: FUSED_KYSELY_MIGRATIONS.len(),
        lock_baseline_version: SQLX_BASELINE_VERSION,
    })
}

async fn bridge_legacy_schema_if_needed(pool: &PgPool) -> Result<(), MigrationError> {
    if sqlx_version_applied(pool, SQLX_BASELINE_VERSION).await? {
        return Ok(());
    }

    if !table_exists(pool, "asset").await? {
        println!("database migrations: empty database; sqlx will apply baseline");
        return Ok(());
    }

    let Some(baseline) = MIGRATOR
        .iter()
        .find(|migration| migration.version == SQLX_BASELINE_VERSION)
    else {
        return Err(MigrationError::Failed(format!(
            "missing sqlx migration version {SQLX_BASELINE_VERSION} ({SQLX_BASELINE_FILE})"
        )));
    };

    println!(
        "database migrations: existing Immich schema detected; \
         bridging kysely/init history → sqlx baseline (version {SQLX_BASELINE_VERSION})"
    );

    let mut conn = pool
        .acquire()
        .await
        .map_err(|err| MigrationError::Failed(err.to_string()))?;

    conn.ensure_migrations_table()
        .await
        .map_err(|err| MigrationError::Failed(err.to_string()))?;

    record_migration_applied(&mut conn, baseline).await?;

    Ok(())
}

/// When an Immich DB already applied the Kysely files that a later sqlx version
/// absorbs (or already has the end-state schema), record that sqlx version
/// without re-executing it.
async fn bridge_incremental_if_kysely_already_applied(pool: &PgPool) -> Result<(), MigrationError> {
    if INCREMENTAL_SQLX_COVERAGE.is_empty() {
        return Ok(());
    }

    let kysely_applied: std::collections::HashSet<String> =
        if table_exists(pool, "kysely_migrations").await? {
            sqlx::query_scalar::<_, String>(r#"SELECT name FROM kysely_migrations"#)
                .fetch_all(pool)
                .await
                .map_err(|err| MigrationError::Failed(err.to_string()))?
                .into_iter()
                .collect()
        } else {
            std::collections::HashSet::new()
        };

    let cluster_present = table_exists(pool, "cluster_group").await?;
    let workflow_log_present = table_exists(pool, "workflow_log").await?;
    let allowed_hosts_present = column_exists(pool, "plugin_method", "allowedHosts").await?;

    let mut conn = pool
        .acquire()
        .await
        .map_err(|err| MigrationError::Failed(err.to_string()))?;

    conn.ensure_migrations_table()
        .await
        .map_err(|err| MigrationError::Failed(err.to_string()))?;

    for coverage in INCREMENTAL_SQLX_COVERAGE {
        if sqlx_version_applied(pool, coverage.sqlx_version).await? {
            continue;
        }

        let all_kysely_present = !coverage.kysely_migrations.is_empty()
            && coverage
                .kysely_migrations
                .iter()
                .all(|name| kysely_applied.contains(*name));

        // End-state heuristic for DBs that already match a later sqlx version
        // without kysely_migrations rows (e.g. restored from a fused dump).
        let end_state_present = cluster_present && workflow_log_present && allowed_hosts_present;

        if !all_kysely_present && !end_state_present {
            continue;
        }

        let Some(migration) = MIGRATOR
            .iter()
            .find(|migration| migration.version == coverage.sqlx_version)
        else {
            return Err(MigrationError::Failed(format!(
                "missing sqlx migration version {}",
                coverage.sqlx_version
            )));
        };

        println!(
            "database migrations: bridging already-applied upstream schema → sqlx version {} ({})",
            coverage.sqlx_version, migration.description
        );
        record_migration_applied(&mut conn, migration).await?;
    }

    Ok(())
}

async fn sqlx_version_applied(pool: &PgPool, version: i64) -> Result<bool, MigrationError> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(false);
    }

    let applied: bool = sqlx::query_scalar(
        r#"
            SELECT EXISTS (
                SELECT 1 FROM _sqlx_migrations
                WHERE version = $1 AND success = true
            )
        "#,
    )
    .bind(version)
    .fetch_one(pool)
    .await
    .map_err(|err| MigrationError::Failed(err.to_string()))?;

    Ok(applied)
}

async fn table_exists(pool: &PgPool, table: &str) -> Result<bool, MigrationError> {
    sqlx::query_scalar(
        r#"
            SELECT EXISTS (
                SELECT 1
                FROM information_schema.tables
                WHERE table_schema = 'public'
                  AND table_name = $1
            )
        "#,
    )
    .bind(table)
    .fetch_one(pool)
    .await
    .map_err(|err| MigrationError::Failed(err.to_string()))
}

async fn column_exists(pool: &PgPool, table: &str, column: &str) -> Result<bool, MigrationError> {
    sqlx::query_scalar(
        r#"
            SELECT EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = 'public'
                  AND table_name = $1
                  AND column_name = $2
            )
        "#,
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await
    .map_err(|err| MigrationError::Failed(err.to_string()))
}

async fn record_migration_applied(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    migration: &Migration,
) -> Result<(), MigrationError> {
    sqlx::query(
        r#"
            INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
            VALUES ($1, $2, TRUE, $3, 0)
            ON CONFLICT (version) DO NOTHING
        "#,
    )
    .bind(migration.version)
    .bind(&*migration.description)
    .bind(&*migration.checksum)
    .execute(&mut **conn)
    .await
    .map_err(|err| MigrationError::Failed(err.to_string()))?;

    Ok(())
}

pub async fn verify_schema(pool: &PgPool) -> Result<(), MigrationError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrator_includes_baseline() {
        let baseline = MIGRATOR
            .iter()
            .find(|migration| migration.version == SQLX_BASELINE_VERSION)
            .expect("expected baseline sqlx migration");
        assert!(
            baseline.description.contains("baseline"),
            "unexpected description: {}",
            baseline.description
        );
        assert!(!baseline.sql.is_empty());
    }

    #[test]
    fn lock_lists_fused_kysely_history() {
        assert_eq!(SQLX_BASELINE_VERSION, 1);
        assert_eq!(SQLX_BASELINE_FILE, "1_baseline.sql");
        assert!(
            FUSED_KYSELY_MIGRATIONS.len() >= 90,
            "expected fused Kysely history in baseline_lock.json, got {}",
            FUSED_KYSELY_MIGRATIONS.len()
        );
        assert!(
            FUSED_KYSELY_MIGRATIONS
                .iter()
                .any(|name| name.contains("InitialMigration"))
        );
        assert!(
            FUSED_KYSELY_MIGRATIONS
                .iter()
                .any(|name| name.contains("ClusterGroups"))
        );
        assert!(
            INCREMENTAL_SQLX_COVERAGE.is_empty(),
            "baseline not locked yet — keep a single sqlx v1 until intentional upstream sync"
        );
        assert_eq!(
            MIGRATOR
                .iter()
                .filter(|m| !m.migration_type.is_down_migration())
                .count(),
            1,
            "expected only sqlx migration version 1 until post-baseline sync"
        );
    }
}
