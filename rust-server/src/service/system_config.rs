use serde_json::Value;
use sqlx::PgPool;

use crate::models::db::system_metadata::{get_json, set_json};
use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::utils::permission::{require_admin, require_permission};
use crate::utils::preferences::merge_preferences;
use crate::service::websocket::WebSocketHub;

const CONFIG_KEY: &str = "system-config";
const DEFAULTS_JSON: &str = include_str!("../../config/system_config_defaults.json");

#[derive(Clone)]
pub struct SystemConfigService {
    pool: PgPool,
    websocket: WebSocketHub,
}

impl SystemConfigService {
    pub fn new(pool: PgPool, websocket: WebSocketHub) -> Self {
        Self { pool, websocket }
    }

    pub fn defaults(&self) -> Value {
        serde_json::from_str(DEFAULTS_JSON).unwrap_or_else(|_| Value::Object(Default::default()))
    }

    pub fn get_defaults(&self, auth: &AuthDto) -> Result<Value, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::SystemConfigRead)?;
        Ok(self.defaults())
    }

    pub async fn get_config(&self, auth: &AuthDto) -> Result<Value, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::SystemConfigRead)?;

        let defaults = self.defaults();
        let stored = get_json(&self.pool, CONFIG_KEY).await?;
        Ok(merge_config(defaults, stored))
    }

    pub async fn update_config(&self, auth: &AuthDto, dto: &Value) -> Result<Value, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::SystemConfigUpdate)?;

        set_json(&self.pool, CONFIG_KEY, dto).await?;
        self.websocket.emit_config_update();
        Ok(dto.clone())
    }

    pub fn storage_template_options(&self, auth: &AuthDto) -> Result<Value, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::SystemConfigRead)?;

        Ok(serde_json::json!({
            "secondOptions": ["s", "ss", "SSS"],
            "minuteOptions": ["m", "mm"],
            "dayOptions": ["d", "dd"],
            "weekOptions": ["W", "WW"],
            "hourOptions": ["h", "hh", "H", "HH"],
            "yearOptions": ["y", "yy"],
            "monthOptions": ["M", "MM", "MMM", "MMMM"],
            "presetOptions": [
                "{{y}}/{{y}}-{{MM}}-{{dd}}/{{filename}}",
                "{{y}}/{{MM}}-{{dd}}/{{filename}}",
                "{{y}}/{{MMMM}}-{{dd}}/{{filename}}",
                "{{y}}/{{MM}}/{{filename}}",
                "{{y}}/{{#if album}}{{album}}{{else}}Other/{{MM}}{{/if}}/{{filename}}",
                "{{#if album}}{{album-startDate-y}}/{{album}}{{else}}{{y}}/Other/{{MM}}{{/if}}/{{filename}}",
                "{{y}}/{{MMM}}/{{filename}}",
                "{{y}}/{{MMMM}}/{{filename}}",
                "{{y}}/{{MM}}/{{dd}}/{{filename}}",
                "{{y}}/{{MMMM}}/{{dd}}/{{filename}}",
                "{{y}}/{{y}}-{{MM}}/{{y}}-{{MM}}-{{dd}}/{{filename}}",
                "{{y}}-{{MM}}-{{dd}}/{{filename}}",
                "{{y}}-{{MMM}}-{{dd}}/{{filename}}",
                "{{y}}-{{MMMM}}-{{dd}}/{{filename}}",
                "{{y}}/{{y}}-{{MM}}/{{filename}}",
                "{{y}}/{{y}}-{{WW}}/{{filename}}",
                "{{y}}/{{y}}-{{MM}}-{{dd}}/{{assetId}}",
                "{{y}}/{{y}}-{{MM}}/{{assetId}}",
                "{{y}}/{{y}}-{{WW}}/{{assetId}}",
                "{{album}}/{{filename}}",
                "{{make}}/{{model}}/{{lensModel}}/{{filename}}"
            ]
        }))
    }
}

fn merge_config(defaults: Value, stored: Option<Value>) -> Value {
    match stored {
        Some(stored) => {
            let mut merged = defaults;
            merge_preferences(&mut merged, stored);
            merged
        }
        None => defaults,
    }
}
