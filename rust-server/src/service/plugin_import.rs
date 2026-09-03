use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::models::db::plugin::{self, PluginMethodUpsertInput, PluginUpsertInput};
use crate::models::dto::env::{EnvDto, ImmichEnvironment};
use crate::utils::crypto::hash_sha256;

const PLUGIN_IMPORT_LOCK: i64 = 666;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginManifest {
    name: String,
    version: String,
    title: String,
    description: String,
    author: String,
    wasm_path: String,
    #[serde(default)]
    methods: Vec<PluginManifestMethod>,
    #[serde(default)]
    templates: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginManifestMethod {
    name: String,
    title: String,
    description: String,
    #[serde(default)]
    types: Vec<String>,
    #[serde(default)]
    host_functions: bool,
    schema: Option<Value>,
    #[serde(default)]
    ui_hints: Vec<String>,
    #[serde(default)]
    allowed_hosts: Vec<String>,
}

pub async fn sync_plugins(pool: &PgPool, env: &EnvDto) -> Result<(), String> {
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(PLUGIN_IMPORT_LOCK)
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;

    let result = sync_plugins_inner(pool, env).await;

    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(PLUGIN_IMPORT_LOCK)
        .execute(pool)
        .await;

    result
}

async fn sync_plugins_inner(pool: &PgPool, env: &EnvDto) -> Result<(), String> {
    let force = matches!(env.immich_env, Some(ImmichEnvironment::Development));

    if let Some(core_plugin) = resolve_core_plugin_path(env) {
        import_folder(pool, &core_plugin, force).await?;
    }

    if env.immich_allow_external_plugins.unwrap_or(false) {
        if let Some(install_folder) = env.immich_plugins_install_folder.as_deref() {
            import_folders(pool, Path::new(install_folder)).await?;
        }
    }

    Ok(())
}

fn resolve_core_plugin_path(env: &EnvDto) -> Option<PathBuf> {
    if let Some(path) = env.immich_core_plugin.as_deref() {
        return Some(PathBuf::from(path));
    }

    if matches!(env.immich_env, Some(ImmichEnvironment::Development)) {
        let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../packages/plugin-core");
        if dev_path.join("manifest.json").exists() {
            return Some(dev_path);
        }
    }

    let prod_path = PathBuf::from("/build/plugins/immich-plugin-core");
    if prod_path.join("manifest.json").exists() {
        return Some(prod_path);
    }

    None
}

async fn import_folders(pool: &PgPool, install_folder: &Path) -> Result<(), String> {
    let mut entries = tokio::fs::read_dir(install_folder).await.map_err(|err| {
        format!(
            "Failed to read plugins folder {}: {err}",
            install_folder.display()
        )
    })?;

    while let Some(entry) = entries.next_entry().await.map_err(|err| err.to_string())? {
        let file_type = entry.file_type().await.map_err(|err| err.to_string())?;
        if file_type.is_dir() {
            import_folder(pool, &entry.path(), false).await?;
        }
    }

    Ok(())
}

async fn import_folder(pool: &PgPool, folder: &Path, force: bool) -> Result<(), String> {
    let manifest_path = folder.join("manifest.json");
    let contents = match tokio::fs::read_to_string(&manifest_path).await {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!("Failed to import plugin from {}: {err}", folder.display());
            return Ok(());
        }
    };

    let sha256hash = hash_sha256(&contents);

    if !force {
        if plugin::get_by_hash(pool, &sha256hash)
            .await
            .map_err(|err| err.to_string())?
            .is_some()
        {
            return Ok(());
        }
    }

    let manifest: PluginManifest = match serde_json::from_str(&contents) {
        Ok(manifest) => manifest,
        Err(err) => {
            eprintln!(
                "Invalid plugin manifest at {}: {err}",
                manifest_path.display()
            );
            return Ok(());
        }
    };

    if manifest.name.is_empty() || manifest.version.is_empty() {
        return Err(format!(
            "Invalid plugin manifest at {}",
            manifest_path.display()
        ));
    }

    let wasm_path = folder.join(&manifest.wasm_path);
    let wasm_bytes = tokio::fs::read(&wasm_path)
        .await
        .map_err(|err| format!("Failed to read wasm at {}: {err}", wasm_path.display()))?;

    let methods: Vec<PluginMethodUpsertInput> = manifest
        .methods
        .into_iter()
        .map(|method| PluginMethodUpsertInput {
            name: method.name,
            title: method.title,
            description: method.description,
            types: method.types,
            host_functions: method.host_functions,
            schema: method
                .schema
                .filter(|schema| !schema.is_null() && schema != &json!({})),
            ui_hints: method.ui_hints,
            allowed_hosts: method.allowed_hosts,
        })
        .collect();

    let existing = plugin::get_by_name(pool, &manifest.name)
        .await
        .map_err(|err| err.to_string())?;

    let (plugin_id, was_update) = plugin::upsert(
        pool,
        &PluginUpsertInput {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            title: manifest.title,
            description: manifest.description,
            author: manifest.author,
            wasm_bytes,
            templates: Value::Array(manifest.templates),
            sha256hash,
        },
        &methods,
    )
    .await
    .map_err(|err| err.to_string())?;

    if was_update {
        if let Some(existing) = existing {
            println!(
                "Upgraded plugin {} from {} to {} ({plugin_id})",
                manifest.name, existing.version, manifest.version
            );
        }
    } else {
        println!(
            "Imported plugin {}@{} ({} methods) from {}",
            manifest.name,
            manifest.version,
            methods.len(),
            folder.display()
        );
    }

    Ok(())
}
