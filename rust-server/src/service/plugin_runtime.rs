use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use extism::{Manifest, Plugin, Wasm};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::plugin::{PluginLoadMethodJson, PluginLoadRow};
use crate::service::job::JobService;
use crate::service::plugin_host::{CallHostContext, HostContext};
use crate::service::websocket::WebSocketHub;
use crate::utils::crypto::random_bytes_as_text;

pub struct PluginRuntime {
    plugins: Mutex<HashMap<String, Plugin>>,
    context: Arc<HostContext>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct WorkflowAuthClaims {
    userId: String,
}

impl PluginRuntime {
    pub fn new(pool: PgPool, jobs: JobService, websocket: WebSocketHub) -> Self {
        let jwt_secret = random_bytes_as_text(32);
        Self {
            plugins: Mutex::new(HashMap::new()),
            context: Arc::new(HostContext {
                pool,
                jobs,
                websocket,
                jwt_secret,
            }),
        }
    }

    pub fn context(&self) -> Arc<HostContext> {
        self.context.clone()
    }

    pub fn load_all(&self, rows: Vec<PluginLoadRow>) {
        for row in rows {
            let methods: Vec<PluginLoadMethodJson> =
                serde_json::from_value(row.methods.clone()).unwrap_or_default();
            let has_non_host = methods.iter().any(|method| !method.host_functions);
            let has_host = methods.iter().any(|method| method.host_functions);

            if has_non_host {
                self.try_load(&row, false);
            }
            if has_host {
                self.try_load(&row, true);
            }
        }
    }

    fn try_load(&self, row: &PluginLoadRow, host_functions: bool) {
        let key = plugin_key(&row.id, host_functions);
        let wasm = Wasm::data(row.wasm_bytes.clone());
        let manifest = Manifest::new([wasm]);
        let stubs = !host_functions;
        let functions = HostContext::host_functions(self.context.clone(), stubs);

        match Plugin::new(manifest, functions, true) {
            Ok(plugin) => {
                let Ok(mut plugins) = self.plugins.lock() else {
                    return;
                };
                plugins.insert(key, plugin);
                let label = if host_functions {
                    format!("{}@{}/worker", row.name, row.version)
                } else {
                    format!("{}@{}", row.name, row.version)
                };
                println!("Loaded workflow plugin: {label}");
            }
            Err(err) => {
                eprintln!(
                    "Unable to load plugin {}@{} (host_functions={host_functions}): {err}",
                    row.name, row.version
                );
            }
        }
    }

    pub fn call_method(
        &self,
        plugin_key: &str,
        method_name: &str,
        input: &Value,
        allowed_hosts: &[String],
    ) -> Result<Value, String> {
        let input_str = serde_json::to_string(input).map_err(|err| err.to_string())?;
        let mut plugins = self.plugins.lock().map_err(|err| err.to_string())?;
        let plugin = plugins
            .get_mut(plugin_key)
            .ok_or_else(|| format!("No loaded plugin found for {plugin_key}"))?;
        let output: String = plugin
            .call_with_host_context(
                method_name,
                input_str.as_str(),
                CallHostContext {
                    allowed_hosts: allowed_hosts.to_vec(),
                },
            )
            .map_err(|err| err.to_string())?;
        if output.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&output).map_err(|err| err.to_string())
    }

    pub fn sign_auth_token(&self, user_id: &Uuid) -> Result<String, String> {
        encode(
            &Header::new(Algorithm::HS256),
            &WorkflowAuthClaims {
                userId: user_id.to_string(),
            },
            &EncodingKey::from_secret(self.context.jwt_secret.as_bytes()),
        )
        .map_err(|err| err.to_string())
    }
}

pub fn plugin_key(id: &Uuid, host_functions: bool) -> String {
    if host_functions {
        format!("{id}/worker")
    } else {
        id.to_string()
    }
}
