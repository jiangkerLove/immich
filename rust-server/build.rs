use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let server_migrations = manifest_dir.join("../server/src/schema/migrations");
    let fallback_file = manifest_dir.join("schema/kysely_migration_names.txt");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_file = out_dir.join("kysely_migrations.rs");

    println!("cargo:rerun-if-changed={}", fallback_file.display());
    if server_migrations.exists() {
        println!("cargo:rerun-if-changed={}", server_migrations.display());
    }

    let names = if server_migrations.is_dir() {
        read_migration_names_from_dir(&server_migrations)
    } else {
        read_migration_names_from_file(&fallback_file)
    };

    let body = names
        .iter()
        .map(|name| format!("    \"{name}\","))
        .collect::<Vec<_>>()
        .join("\n");
    let content = format!("pub const KYSELY_MIGRATION_NAMES: &[&str] = &[\n{body}\n];\n");
    fs::write(out_file, content).expect("failed to write kysely_migrations.rs");
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
