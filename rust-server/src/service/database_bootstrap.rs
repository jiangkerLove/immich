use sqlx::PgPool;

use crate::models::db::advisory_lock;
use crate::models::dto::env::{DbVectorExtension, EnvDto};
use crate::utils::vector::{get_dimension_size, resolve_vector_extension};

pub const LOCK_MIGRATIONS: i64 = 200;
pub const LOCK_GEODATA_IMPORT: i64 = 100;

const POSTGRES_VERSION_RANGE: &str = ">=14.0.0";
const VECTOR_VERSION_RANGE: &str = ">=0.5 <1";
const VECTORCHORD_VERSION_RANGE: &str = ">=0.3 <2";
const VECTORCHORD_LIST_SLACK_FACTOR: f64 = 1.2;

const VECTOR_EXTENSION_NAMES: &[&str] = &["vector", "vchord", "vchordrq", "vectors"];

struct ExtensionVersion {
    name: String,
    available_version: Option<String>,
    installed_version: Option<String>,
}

pub async fn on_startup(pool: &PgPool, env: &EnvDto) -> Result<(), String> {
    check_postgres_version(pool).await?;

    let Some(_lock) = advisory_lock::try_acquire(pool, LOCK_MIGRATIONS)
        .await
        .map_err(|err| err.to_string())?
    else {
        tracing::info!(
            "database bootstrap: migration lock held by another instance, skipping extension setup"
        );
        return Ok(());
    };

    let extension = setup_vector_extension(pool, env).await?;
    reindex_vectors_if_needed(pool, &extension).await?;
    drop_unused_extensions(pool, &extension).await?;
    prewarm_vector_indexes(pool, &extension).await?;

    Ok(())
}

pub async fn log_schema_drift(pool: &PgPool) {
    match crate::models::db::schema_check::run(pool).await {
        Ok(report) => {
            if crate::models::db::schema_check::print_report(&report) {
                tracing::error!(
                    "database bootstrap: schema drift detected; run `immich-admin schema-check` for details"
                );
            } else {
                tracing::info!("database bootstrap: no schema drift detected");
            }
        }
        Err(err) => tracing::error!("database bootstrap: schema drift check failed: {err}"),
    }
}

async fn check_postgres_version(pool: &PgPool) -> Result<(), String> {
    let version: String = sqlx::query_scalar("SHOW server_version")
        .fetch_one(pool)
        .await
        .map_err(|err| err.to_string())?;
    if !version_satisfies(&normalize_server_version(&version), POSTGRES_VERSION_RANGE) {
        return Err(format!(
            "Invalid PostgreSQL version. Found {version}, but needed {POSTGRES_VERSION_RANGE}. Please use a supported version."
        ));
    }
    Ok(())
}

async fn setup_vector_extension(pool: &PgPool, env: &EnvDto) -> Result<DbVectorExtension, String> {
    let extension = resolve_vector_extension(pool, env.db_vector_extension.clone()).await?;
    let extension_name = extension_sql_name(&extension);
    let display_name = extension_display_name(&extension);
    let extension_range = extension_version_range(&extension);

    let versions = get_extension_versions(pool).await?;
    let current = versions
        .iter()
        .find(|item| item.name == extension_name)
        .ok_or_else(|| {
            format!("The {display_name} extension is not available in this Postgres instance.")
        })?;

    let available_version = current.available_version.as_deref().ok_or_else(|| {
        format!("The {display_name} extension is not available in this Postgres instance.")
    })?;

    if is_nightly_version(available_version)
        || current
            .installed_version
            .as_deref()
            .is_some_and(is_nightly_version)
    {
        return Err(format!(
            "The {display_name} extension version is 0.0.0, which means it is a nightly release."
        ));
    }

    if !version_satisfies(available_version, extension_range) {
        return Err(format!(
            "The {display_name} extension version is {available_version}, but Immich only supports {extension_range}."
        ));
    }

    if current.installed_version.is_none() {
        create_extension(pool, &extension).await?;
    }

    if let Some(installed_version) = current.installed_version.as_deref() {
        if version_gt(available_version, installed_version) {
            tracing::info!(
                "database bootstrap: updating {display_name} extension to {available_version}"
            );
            drop_vector_indexes(pool).await?;
            update_extension(pool, extension_name, available_version).await?;
            reindex_vectors_if_needed(pool, &extension).await?;
        } else if !version_satisfies(installed_version, extension_range) {
            return Err(format!(
                "The {display_name} extension version is {installed_version}, but Immich only supports {extension_range}."
            ));
        } else if version_lt(available_version, installed_version) {
            return Err(format!(
                "The database currently has {display_name} {installed_version} activated, but the Postgres instance only has {available_version} available."
            ));
        }
    }

    Ok(extension)
}

async fn create_extension(pool: &PgPool, extension: &DbVectorExtension) -> Result<(), String> {
    let extension_name = extension_sql_name(extension);
    let display_name = extension_display_name(extension);
    let sql = format!("CREATE EXTENSION IF NOT EXISTS {extension_name} CASCADE");
    tracing::info!("database bootstrap: creating {display_name} extension");
    sqlx::query(&sql)
        .execute(pool)
        .await
        .map_err(|err| format!("Failed to activate {display_name} extension: {err}"))?;

    if matches!(extension, DbVectorExtension::VectorChord) {
        let db_name: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(pool)
            .await
            .map_err(|err| err.to_string())?;
        sqlx::query(&format!(
            "ALTER DATABASE \"{db_name}\" SET vchordrq.probes = 1"
        ))
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;
        sqlx::query("SET vchordrq.probes = 1")
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
    }

    Ok(())
}

async fn update_extension(
    pool: &PgPool,
    extension_name: &str,
    target_version: &str,
) -> Result<(), String> {
    let sql = format!(
        "ALTER EXTENSION {extension_name} UPDATE TO '{}'",
        target_version.replace('\'', "''")
    );
    sqlx::query(&sql)
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

async fn drop_unused_extensions(
    pool: &PgPool,
    extension: &DbVectorExtension,
) -> Result<(), String> {
    let active = extension_sql_name(extension);
    for item in get_extension_versions(pool).await? {
        let Some(installed_version) = item.installed_version else {
            continue;
        };
        if should_keep_extension(&item.name, active) {
            continue;
        }
        if !VECTOR_EXTENSION_NAMES.contains(&item.name.as_str()) {
            continue;
        }

        tracing::info!(
            "database bootstrap: dropping unused extension {} ({installed_version})",
            item.name
        );
        if let Err(err) = sqlx::query(&format!("DROP EXTENSION IF EXISTS {}", item.name))
            .execute(pool)
            .await
        {
            tracing::error!(
                "database bootstrap: could not drop extension {}: {err}",
                item.name
            );
        }
    }

    Ok(())
}

async fn prewarm_vector_indexes(
    pool: &PgPool,
    extension: &DbVectorExtension,
) -> Result<(), String> {
    if !matches!(extension, DbVectorExtension::VectorChord) {
        return Ok(());
    }

    for index_name in ["clip_index", "face_index"] {
        if let Err(err) = sqlx::query(&format!("SELECT vchordrq_prewarm('{index_name}')"))
            .execute(pool)
            .await
        {
            tracing::error!("database bootstrap: prewarm {index_name} failed: {err}");
        } else {
            tracing::info!("database bootstrap: prewarmed {index_name}");
        }
    }

    Ok(())
}

async fn reindex_vectors_if_needed(
    pool: &PgPool,
    extension: &DbVectorExtension,
) -> Result<(), String> {
    for (index_name, table) in [
        ("clip_index", "smart_search"),
        ("face_index", "face_search"),
    ] {
        if !table_exists(pool, table).await? {
            continue;
        }

        let indexdef: Option<String> =
            sqlx::query_scalar("SELECT indexdef FROM pg_indexes WHERE indexname = $1")
                .bind(index_name)
                .fetch_optional(pool)
                .await
                .map_err(|err| err.to_string())?;

        let needs_reindex = match indexdef {
            None => true,
            Some(def) => match extension {
                DbVectorExtension::PgVector => !def.to_lowercase().contains("using hnsw"),
                DbVectorExtension::VectorChord => {
                    !def.to_lowercase().contains("using vchordrq")
                        || vectorchord_lists_mismatch(pool, table, &def).await?
                }
                DbVectorExtension::PgvectoRs => !def.to_lowercase().contains("using vectors"),
            },
        };

        if needs_reindex {
            reindex_vector_index(pool, index_name, table, extension).await?;
        }
    }

    Ok(())
}

async fn vectorchord_lists_mismatch(
    pool: &PgPool,
    table: &str,
    indexdef: &str,
) -> Result<bool, String> {
    let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .map_err(|err| err.to_string())?;
    let target_lists = target_list_count(count as u64);
    let current_lists = parse_vectorchord_lists(indexdef).unwrap_or(1);
    let slack_lists = target_list_count((count as f64 * VECTORCHORD_LIST_SLACK_FACTOR) as u64);
    Ok(current_lists != target_lists && current_lists != slack_lists)
}

fn target_list_count(count: u64) -> u64 {
    if count < 128_000 {
        1
    } else if count < 2_048_000 {
        let value = (count / 1000).max(1) as u32;
        1u64 << (32 - value.leading_zeros())
    } else {
        let value = (count as f64).sqrt().max(1.0) as u32;
        1u64 << (33 - value.leading_zeros())
    }
}

fn parse_vectorchord_lists(indexdef: &str) -> Option<u64> {
    let lower = indexdef.to_lowercase();
    let start = lower.find("lists = [")?;
    let rest = &lower[start + 9..];
    let end = rest.find(']')?;
    rest[..end].trim().parse().ok()
}

async fn reindex_vector_index(
    pool: &PgPool,
    index_name: &str,
    table: &str,
    extension: &DbVectorExtension,
) -> Result<(), String> {
    tracing::info!("database bootstrap: reindexing {index_name} (this may take a while)");

    let dim_size = get_dimension_size(pool, table, "embedding").await;
    let lists = if matches!(extension, DbVectorExtension::VectorChord) {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await
            .map_err(|err| err.to_string())?;
        target_list_count(count.max(0) as u64)
    } else {
        1
    };

    let index_sql = vector_index_sql(index_name, extension, lists);

    let mut tx = pool.begin().await.map_err(|err| err.to_string())?;
    sqlx::query(&format!("DROP INDEX IF EXISTS {index_name}"))
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;

    if table == "smart_search" {
        sqlx::query("ALTER TABLE smart_search DROP CONSTRAINT IF EXISTS dim_size_constraint")
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
    }

    sqlx::query(&format!(
        "ALTER TABLE {table} ALTER COLUMN embedding TYPE vector({dim_size})"
    ))
    .execute(&mut *tx)
    .await
    .map_err(|err| err.to_string())?;
    sqlx::query(&index_sql)
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    tx.commit().await.map_err(|err| err.to_string())?;

    let _ = sqlx::query(&format!("VACUUM ANALYZE {table}"))
        .execute(pool)
        .await;

    tracing::info!("database bootstrap: reindexed {index_name}");
    Ok(())
}

fn vector_index_sql(index_name: &str, extension: &DbVectorExtension, lists: u64) -> String {
    match (index_name, extension) {
        ("clip_index", DbVectorExtension::VectorChord) => format!(
            r#"
            CREATE INDEX IF NOT EXISTS clip_index ON smart_search
            USING vchordrq (embedding vector_cosine_ops) WITH (options = $$
            residual_quantization = false
            [build.internal]
            lists = [{lists}]
            spherical_centroids = true
            build_threads = 4
            sampling_factor = 1024
            $$)
            "#
        ),
        ("face_index", DbVectorExtension::VectorChord) => format!(
            r#"
            CREATE INDEX IF NOT EXISTS face_index ON face_search
            USING vchordrq (embedding vector_cosine_ops) WITH (options = $$
            residual_quantization = false
            [build.internal]
            lists = [{lists}]
            spherical_centroids = true
            build_threads = 4
            sampling_factor = 1024
            $$)
            "#
        ),
        ("clip_index", DbVectorExtension::PgVector | DbVectorExtension::PgvectoRs) => r#"
            CREATE INDEX IF NOT EXISTS clip_index ON smart_search
            USING hnsw (embedding vector_cosine_ops)
            WITH (ef_construction = 300, m = 16)
            "#
        .to_string(),
        _ => r#"
            CREATE INDEX IF NOT EXISTS face_index ON face_search
            USING hnsw (embedding vector_cosine_ops)
            WITH (ef_construction = 300, m = 16)
            "#
        .to_string(),
    }
}

async fn drop_vector_indexes(pool: &PgPool) -> Result<(), String> {
    for index in ["clip_index", "face_index"] {
        sqlx::query(&format!("DROP INDEX IF EXISTS {index}"))
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

async fn get_extension_versions(pool: &PgPool) -> Result<Vec<ExtensionVersion>, String> {
    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT
            name,
            default_version AS available_version,
            installed_version
        FROM pg_available_extensions
        WHERE name IN ('vector', 'vchord', 'vchordrq', 'vectors')
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|err| err.to_string())?;

    Ok(rows
        .into_iter()
        .map(
            |(name, available_version, installed_version)| ExtensionVersion {
                name,
                available_version,
                installed_version,
            },
        )
        .collect())
}

async fn table_exists(pool: &PgPool, table: &str) -> Result<bool, String> {
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
    .map(|value| value)
    .map_err(|err| err.to_string())
}

fn should_keep_extension(installed_name: &str, active_name: &str) -> bool {
    installed_name == active_name
        || (installed_name == "vector"
            && (active_name == "vchord" || active_name == "vchordrq"))
        // Legacy mis-name: some builds listed the access method as an extension candidate.
        || (installed_name == "vchordrq" && active_name == "vchord")
        || (installed_name == "vchord" && active_name == "vchordrq")
}

fn extension_sql_name(extension: &DbVectorExtension) -> &'static str {
    match extension {
        DbVectorExtension::PgVector => "vector",
        // Upstream Immich / VectorChord: CREATE EXTENSION vchord; indexes use USING vchordrq.
        DbVectorExtension::VectorChord => "vchord",
        DbVectorExtension::PgvectoRs => "vectors",
    }
}

fn extension_display_name(extension: &DbVectorExtension) -> &'static str {
    match extension {
        DbVectorExtension::PgVector => "pgvector",
        DbVectorExtension::VectorChord => "VectorChord",
        DbVectorExtension::PgvectoRs => "pgvecto.rs",
    }
}

fn extension_version_range(extension: &DbVectorExtension) -> &'static str {
    match extension {
        DbVectorExtension::VectorChord => VECTORCHORD_VERSION_RANGE,
        DbVectorExtension::PgVector | DbVectorExtension::PgvectoRs => VECTOR_VERSION_RANGE,
    }
}

fn normalize_server_version(version: &str) -> String {
    let core = version.split_whitespace().next().unwrap_or(version);
    match semver::Version::parse(core) {
        Ok(parsed) => parsed.to_string(),
        Err(_) => {
            let parts: Vec<&str> = core.split('.').collect();
            match parts.len() {
                1 => format!("{}.0.0", parts[0]),
                2 => format!("{}.{}.0", parts[0], parts[1]),
                _ => core.to_string(),
            }
        }
    }
}

fn version_satisfies(version: &str, range: &str) -> bool {
    let Ok(req) = semver::VersionReq::parse(range) else {
        return false;
    };
    semver::Version::parse(version)
        .map(|parsed| req.matches(&parsed))
        .unwrap_or(false)
}

fn is_nightly_version(version: &str) -> bool {
    semver::Version::parse(version).is_ok_and(|parsed| parsed == semver::Version::new(0, 0, 0))
}

fn version_gt(left: &str, right: &str) -> bool {
    match (semver::Version::parse(left), semver::Version::parse(right)) {
        (Ok(left), Ok(right)) => left > right,
        _ => left != right,
    }
}

fn version_lt(left: &str, right: &str) -> bool {
    match (semver::Version::parse(left), semver::Version::parse(right)) {
        (Ok(left), Ok(right)) => left < right,
        _ => false,
    }
}
