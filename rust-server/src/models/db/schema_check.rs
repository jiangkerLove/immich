use std::collections::HashSet;

use sqlx::{Pool, Postgres};

const INIT_SQL: &str = include_str!("../../../schema/init.sql");
include!(concat!(env!("OUT_DIR"), "/kysely_migrations.rs"));

const OPTIONAL_TABLES: &[&str] = &["smart_search", "face_search"];
const IGNORED_EXTRA_TABLES: &[&str] = &[
    "kysely_migrations",
    "migration_overrides",
    "cluster_group",
    "cluster_group_request",
    "person_group",
    "person_group_audit",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonSchemaVariant {
    Legacy,
    ClusterGroups,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationStatus {
    Applied,
    Missing,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationCheck {
    pub name: String,
    pub status: MigrationStatus,
}

#[derive(Debug, Default)]
pub struct SchemaCheckReport {
    pub missing_tables: Vec<String>,
    pub extra_tables: Vec<String>,
    pub optional_missing: Vec<String>,
    pub vector_extension: bool,
    pub migrations: Option<Vec<MigrationCheck>>,
}

pub fn expected_tables() -> HashSet<String> {
    let re = regex::Regex::new(r#"CREATE TABLE\s+(?:"([^"]+)"|([a-z_]+))"#).unwrap();
    INIT_SQL
        .lines()
        .filter_map(|line| {
            re.captures(line.trim()).map(|caps| {
                caps.get(1)
                    .or_else(|| caps.get(2))
                    .map(|m| m.as_str().to_string())
                    .expect("table name capture")
            })
        })
        .collect()
}

pub fn expected_migration_names() -> &'static [&'static str] {
    KYSELY_MIGRATION_NAMES
}

pub fn compare_migrations(expected: &[&str], applied: &[String]) -> Vec<MigrationCheck> {
    let files_set: HashSet<&str> = expected.iter().copied().collect();
    let rows_set: HashSet<&str> = applied.iter().map(String::as_str).collect();

    let mut combined: Vec<String> = files_set
        .union(&rows_set)
        .map(|name| (*name).to_string())
        .collect();
    combined.sort();

    combined
        .into_iter()
        .map(|name| {
            let in_files = files_set.contains(name.as_str());
            let in_rows = rows_set.contains(name.as_str());
            let status = match (in_files, in_rows) {
                (true, true) => MigrationStatus::Applied,
                (true, false) => MigrationStatus::Missing,
                (false, true) => MigrationStatus::Deleted,
                (false, false) => MigrationStatus::Applied,
            };
            MigrationCheck { name, status }
        })
        .collect()
}

pub async fn detect_person_schema_variant(
    pool: &Pool<Postgres>,
) -> Result<PersonSchemaVariant, sqlx::Error> {
    let has_person_group: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = 'public'
              AND table_name = 'person_group'
        )
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(if has_person_group {
        PersonSchemaVariant::ClusterGroups
    } else {
        PersonSchemaVariant::Legacy
    })
}

pub async fn run(pool: &Pool<Postgres>) -> Result<SchemaCheckReport, sqlx::Error> {
    let expected = expected_tables();
    let actual = list_public_tables(pool).await?;
    let optional: HashSet<&str> = OPTIONAL_TABLES.iter().copied().collect();
    let ignored: HashSet<&str> = IGNORED_EXTRA_TABLES.iter().copied().collect();

    let mut missing_tables: Vec<String> = expected
        .difference(&actual)
        .filter(|table| !optional.contains(table.as_str()))
        .cloned()
        .collect();
    missing_tables.sort();

    let optional_missing: Vec<String> = OPTIONAL_TABLES
        .iter()
        .filter(|table| !actual.contains(**table))
        .map(|table| (*table).to_string())
        .collect();

    let mut extra_tables: Vec<String> = actual
        .difference(&expected)
        .filter(|table| !ignored.contains(table.as_str()))
        .cloned()
        .collect();
    extra_tables.sort();

    let vector_extension = extension_installed(pool, "vector").await?;
    let migrations = kysely_migration_status(pool).await?;

    Ok(SchemaCheckReport {
        missing_tables,
        extra_tables,
        optional_missing,
        vector_extension,
        migrations,
    })
}

async fn list_public_tables(pool: &Pool<Postgres>) -> Result<HashSet<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'public'
          AND table_type = 'BASE TABLE'
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(name,)| name).collect())
}

async fn extension_installed(pool: &Pool<Postgres>, name: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM pg_extension WHERE extname = $1
        )
        "#,
    )
    .bind(name)
    .fetch_one(pool)
    .await
}

async fn kysely_migration_status(
    pool: &Pool<Postgres>,
) -> Result<Option<Vec<MigrationCheck>>, sqlx::Error> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = 'public'
              AND table_name = 'kysely_migrations'
        )
        "#,
    )
    .fetch_one(pool)
    .await?;

    if !exists {
        return Ok(None);
    }

    let applied: Vec<(String,)> =
        sqlx::query_as(r#"SELECT name FROM kysely_migrations ORDER BY name ASC"#)
            .fetch_all(pool)
            .await?;

    let applied_names = applied.into_iter().map(|(name,)| name).collect::<Vec<_>>();
    Ok(Some(compare_migrations(
        expected_migration_names(),
        &applied_names,
    )))
}

pub fn print_report(report: &SchemaCheckReport) -> bool {
    let mut ok = true;

    match &report.migrations {
        Some(migrations) if migrations.iter().all(|m| m.status == MigrationStatus::Applied) => {
            println!("Migrations are up to date");
        }
        Some(migrations) => {
            ok = false;
            println!("Migration issues detected:");
            for migration in migrations {
                match migration.status {
                    MigrationStatus::Applied => {}
                    MigrationStatus::Missing => {
                        println!(
                            "  - {} exists on disk, but has not been applied to the database",
                            migration.name
                        );
                    }
                    MigrationStatus::Deleted => {
                        println!(
                            "  - {} was applied, but the file no longer exists on disk",
                            migration.name
                        );
                    }
                }
            }
        }
        None => {
            println!("No kysely_migrations table (init.sql bootstrap database)");
            if !expected_migration_names().is_empty() {
                println!(
                    "  Expected {} NestJS migration(s) when upgrading from immich server",
                    expected_migration_names().len()
                );
            }
        }
    }

    if report.missing_tables.is_empty() && report.extra_tables.is_empty() {
        println!("\nNo schema drift detected");
    } else {
        ok = false;
        if !report.missing_tables.is_empty() {
            println!("\nMissing tables:");
            for table in &report.missing_tables {
                println!("  - {table}");
            }
        }
        if !report.extra_tables.is_empty() {
            println!("\nUnexpected tables:");
            for table in &report.extra_tables {
                println!("  - {table}");
            }
        }
    }

    if report.optional_missing.is_empty() {
        println!("\nOptional pgvector tables: present");
    } else {
        println!(
            "\nOptional pgvector tables missing: {}",
            report.optional_missing.join(", ")
        );
        if !report.vector_extension {
            println!("  (pgvector extension is not installed)");
        }
    }

    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_expected_tables_from_init_sql() {
        let tables = expected_tables();
        assert!(tables.contains("user"));
        assert!(tables.contains("asset"));
        assert!(tables.contains("smart_search"));
        assert!(tables.contains("face_search"));
        assert!(tables.len() >= 60);
    }

    #[test]
    fn loads_kysely_migration_names() {
        assert!(!expected_migration_names().is_empty());
        assert!(expected_migration_names()
            .iter()
            .any(|name| name.contains("InitialMigration")));
    }

    #[test]
    fn compare_migrations_detects_missing_and_deleted() {
        let expected = ["100-First", "200-Second"];
        let applied = vec!["100-First".to_string(), "300-Old".to_string()];
        let result = compare_migrations(&expected, &applied);

        assert_eq!(
            result
                .iter()
                .find(|item| item.name == "200-Second")
                .map(|item| item.status),
            Some(MigrationStatus::Missing)
        );
        assert_eq!(
            result
                .iter()
                .find(|item| item.name == "300-Old")
                .map(|item| item.status),
            Some(MigrationStatus::Deleted)
        );
        assert_eq!(
            result
                .iter()
                .find(|item| item.name == "100-First")
                .map(|item| item.status),
            Some(MigrationStatus::Applied)
        );
    }

    #[test]
    fn cluster_group_tables_are_ignored_as_extra() {
        assert!(IGNORED_EXTRA_TABLES.contains(&"cluster_group"));
        assert!(IGNORED_EXTRA_TABLES.contains(&"person_group"));
    }
}
