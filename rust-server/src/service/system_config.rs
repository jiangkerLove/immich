use serde_json::Value;
use sqlx::PgPool;

use crate::models::db::system_metadata::set_json;
use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::utils::permission::{require_admin, require_permission};
use crate::utils::system_config::{defaults, get_merged};
use crate::service::websocket::WebSocketHub;

const CONFIG_KEY: &str = "system-config";

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
        defaults()
    }

    pub fn get_defaults(&self, auth: &AuthDto) -> Result<Value, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::SystemConfigRead)?;
        Ok(defaults())
    }

    pub async fn get_config(&self, auth: &AuthDto) -> Result<Value, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::SystemConfigRead)?;
        get_merged(&self.pool).await.map_err(ErrorResp::from)
    }

    pub async fn update_config(&self, auth: &AuthDto, dto: &Value) -> Result<Value, ErrorResp> {
        require_admin(auth)?;
        require_permission(auth, Permission::SystemConfigUpdate)?;

        set_json(&self.pool, CONFIG_KEY, dto).await?;
        crate::service::config_bootstrap::on_config_update(&self.pool).await;
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
