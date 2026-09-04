//! Emit compile-time constants for:
//! - upstream Kysely migration filenames (from `server/` or fallback txt)
//! - sqlx baseline lock (`migrations/baseline_lock.json`) — what this fork
//!   claims is fused into `1_baseline.sql`

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let server_migrations = manifest_dir.join("../server/src/schema/migrations");
    let fallback_file = manifest_dir.join("schema/kysely_migration_names.txt");
    let lock_file = manifest_dir.join("migrations/baseline_lock.json");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed={}", fallback_file.display());
    println!("cargo:rerun-if-changed={}", lock_file.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("migrations").display()
    );
    if server_migrations.exists() {
        println!("cargo:rerun-if-changed={}", server_migrations.display());
    }

    let upstream_names = if server_migrations.is_dir() {
        read_migration_names_from_dir(&server_migrations)
    } else {
        read_migration_names_from_file(&fallback_file)
    };

    let lock = read_baseline_lock(&lock_file);
    warn_if_upstream_ahead(&upstream_names, &lock.fused_kysely_migrations);

    write_kysely_names_rs(&out_dir.join("kysely_migrations.rs"), &upstream_names);
    write_baseline_lock_rs(&out_dir.join("baseline_lock.rs"), &lock);
}

#[derive(Debug)]
struct IncrementalCoverage {
    sqlx_version: i64,
    kysely_migrations: Vec<String>,
}

#[derive(Debug)]
struct BaselineLock {
    sqlx_baseline_version: i64,
    sqlx_baseline_file: String,
    fused_kysely_migrations: Vec<String>,
    incremental: Vec<IncrementalCoverage>,
}

fn read_baseline_lock(path: &Path) -> BaselineLock {
    let raw = fs::read_to_string(path).unwrap_or_else(|err| {
        panic!("missing migrations/baseline_lock.json ({err}); required for sqlx/Kysely parity tracking")
    });
    let value: serde_json::Value =
        serde_json::from_str(&raw).expect("baseline_lock.json must be valid JSON");

    let sqlx_baseline_version = value
        .get("sqlx_baseline_version")
        .and_then(|v| v.as_i64())
        .expect("sqlx_baseline_version");
    let sqlx_baseline_file = value
        .get("sqlx_baseline_file")
        .and_then(|v| v.as_str())
        .unwrap_or("1_baseline.sql")
        .to_string();
    let fused = value
        .get("fused_kysely_migrations")
        .and_then(|v| v.as_array())
        .expect("fused_kysely_migrations array")
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    let incremental = value
        .get("incremental")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let sqlx_version = item.get("sqlx_version")?.as_i64()?;
                    let kysely_migrations = item
                        .get("kysely_migrations")?
                        .as_array()?
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect::<Vec<_>>();
                    Some(IncrementalCoverage {
                        sqlx_version,
                        kysely_migrations,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    BaselineLock {
        sqlx_baseline_version,
        sqlx_baseline_file,
        fused_kysely_migrations: fused,
        incremental,
    }
}

fn warn_if_upstream_ahead(upstream: &[String], fused: &[String]) {
    let fused_set: std::collections::HashSet<&str> = fused.iter().map(String::as_str).collect();
    let ahead: Vec<&str> = upstream
        .iter()
        .map(String::as_str)
        .filter(|name| !fused_set.contains(name))
        .collect();
    if ahead.is_empty() {
        return;
    }
    println!(
        "cargo:warning=upstream Kysely has {} migration(s) not in migrations/baseline_lock.json — add sqlx migrations/N_*.sql (or refresh fused baseline) then update the lock: {}",
        ahead.len(),
        ahead.join(", ")
    );
}

fn write_kysely_names_rs(out_file: &Path, names: &[String]) {
    let body = names
        .iter()
        .map(|name| format!("    \"{name}\","))
        .collect::<Vec<_>>()
        .join("\n");
    let content = format!("pub const KYSELY_MIGRATION_NAMES: &[&str] = &[\n{body}\n];\n");
    fs::write(out_file, content).expect("failed to write kysely_migrations.rs");
}

fn write_baseline_lock_rs(out_file: &Path, lock: &BaselineLock) {
    let body = lock
        .fused_kysely_migrations
        .iter()
        .map(|name| format!("    \"{name}\","))
        .collect::<Vec<_>>()
        .join("\n");
    let incremental_entries = lock
        .incremental
        .iter()
        .map(|inc| {
            let names = inc
                .kysely_migrations
                .iter()
                .map(|name| format!("            \"{name}\","))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "    IncrementalSqlxCoverage {{\n        sqlx_version: {},\n        kysely_migrations: &[\n{names}\n        ],\n    }},",
                inc.sqlx_version
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let content = format!(
        r#"pub struct IncrementalSqlxCoverage {{
    pub sqlx_version: i64,
    pub kysely_migrations: &'static [&'static str],
}}

pub const SQLX_BASELINE_VERSION: i64 = {version};
pub const SQLX_BASELINE_FILE: &str = "{file}";
pub const FUSED_KYSELY_MIGRATIONS: &[&str] = &[
{body}
];
pub const INCREMENTAL_SQLX_COVERAGE: &[IncrementalSqlxCoverage] = &[
{incremental}
];
"#,
        version = lock.sqlx_baseline_version,
        file = lock.sqlx_baseline_file,
        body = body,
        incremental = incremental_entries,
    );
    fs::write(out_file, content).expect("failed to write baseline_lock.rs");
}

fn read_migration_names_from_dir(dir: &Path) -> Vec<String> {
    let mut names = fs::read_dir(dir)
        .expect("failed to read migrations directory")
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_name = entry.file_name().to_string_lossy().to_string();
            file_name.strip_suffix(".ts").map(str::to_string)
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn read_migration_names_from_file(path: &Path) -> Vec<String> {
    if !path.exists() {
        return Vec::new();
    }

    fs::read_to_string(path)
        .expect("failed to read kysely_migration_names.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}
