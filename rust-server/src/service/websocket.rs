use axum::http::HeaderMap;
use redis::RedisError;
use serde::Serialize;
use socketioxide::SocketIo;
use socketioxide::TransportType;
use socketioxide::adapter::Adapter;
use socketioxide::adapter::Emitter;
use socketioxide::extract::{Extension, HttpExtension, SocketRef, State};
use socketioxide::handler::ConnectHandler;
use socketioxide::layer::SocketIoLayer;
use socketioxide_redis::{CustomRedisAdapter, RedisAdapterConfig, RedisAdapterCtr};

use crate::service::websocket_redis::{ImmichRedisDriver, connect_driver};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::dto::auth::AuthDto;
use crate::models::response::asset::AssetResponse;
use crate::models::response::notification::NotificationResponse;
use crate::models::response::sync::{AssetEditReadyV2, AssetUploadReadyV2};
use crate::service::auth::AuthService;
use crate::service::server::ServerService;
use crate::utils::headers::{extract_auth_tokens, get_shared_link_tokens};

const WS_PATH: &str = "/api/socket.io";

type AppSocketAdapter = CustomRedisAdapter<Emitter, ImmichRedisDriver>;
type AppSocketIo = SocketIo<AppSocketAdapter>;
pub type AppSocketIoLayer = SocketIoLayer<AppSocketAdapter>;

#[derive(Clone)]
pub struct WebsocketAppState {
    pub auth: AuthService,
    pub pool: PgPool,
}

#[derive(Clone)]
pub struct WebSocketHub {
    io: AppSocketIo,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRestartEvent {
    pub is_maintenance_mode: bool,
}

impl WebSocketHub {
    pub fn new(io: AppSocketIo) -> Self {
        Self { io }
    }

    pub async fn build(
        auth: AuthService,
        pool: PgPool,
        redis_url: &str,
    ) -> Result<(AppSocketIoLayer, Self), RedisError> {
        let driver = connect_driver(redis_url).await?;
        let adapter = RedisAdapterCtr::new_with_driver(driver, RedisAdapterConfig::default());
        let app_state = WebsocketAppState {
            auth: auth.clone(),
            pool,
        };

        let (layer, io) = SocketIo::builder()
            .req_path(WS_PATH)
            .transports([TransportType::Websocket])
            .with_state(app_state)
            .with_adapter::<CustomRedisAdapter<_, _>>(adapter)
            .build_layer();

        let hub = Self::new(io.clone());

        let _ = io.ns("/", on_connect.with(auth_middleware));

        Ok((layer, hub))
    }

    pub fn client_send<T: Serialize + Send + Sync + 'static>(
        &self,
        event: &'static str,
        room: impl Into<String>,
        data: T,
    ) {
        let io = self.io.clone();
        let room = room.into();
        tokio::spawn(async move {
            let _ = io.to(room).emit(event, &data).await;
        });
    }

    pub fn client_send_empty(&self, event: &'static str, room: impl Into<String>) {
        let io = self.io.clone();
        let room = room.into();
        tokio::spawn(async move {
            let _ = io.to(room).emit(event, &()).await;
        });
    }

    pub fn client_broadcast<T: Serialize + Send + Sync + 'static>(
        &self,
        event: &'static str,
        data: T,
    ) {
        let io = self.io.clone();
        tokio::spawn(async move {
            let _ = io.emit(event, &data).await;
        });
    }

    pub fn client_broadcast_empty(&self, event: &'static str) {
        let io = self.io.clone();
        tokio::spawn(async move {
            let _ = io.emit(event, &()).await;
        });
    }

    pub fn emit_app_restart(&self, is_maintenance_mode: bool) {
        self.client_broadcast(
            "AppRestartV1",
            AppRestartEvent {
                is_maintenance_mode,
            },
        );
    }

    pub fn emit_maintenance_status(
        &self,
        status: &crate::models::dto::maintenance::MaintenanceStatusResp,
    ) {
        self.client_send("MaintenanceStatusV1", "private", status.clone());
        let public = crate::service::maintenance::public_maintenance_status(status);
        self.client_send("MaintenanceStatusV1", "public", public);
    }

    pub fn emit_maintenance_end(&self) {
        self.client_broadcast(
            "AppRestartV1",
            AppRestartEvent {
                is_maintenance_mode: false,
            },
        );
    }

    pub fn emit_asset_trash(&self, user_id: Uuid, asset_ids: Vec<String>) {
        self.client_send("on_asset_trash", user_id.to_string(), asset_ids);
    }

    pub fn emit_asset_delete(&self, user_id: Uuid, asset_id: Uuid) {
        self.client_send("on_asset_delete", user_id.to_string(), asset_id.to_string());
    }

    pub fn emit_asset_restore(&self, user_id: Uuid, asset_ids: Vec<String>) {
        self.client_send("on_asset_restore", user_id.to_string(), asset_ids);
    }

    pub fn emit_stack_update(&self, user_id: Uuid) {
        self.client_send_empty("on_asset_stack_update", user_id.to_string());
    }

    pub fn emit_config_update(&self) {
        self.client_broadcast_empty("on_config_update");
    }

    pub fn emit_user_delete(&self, user_id: Uuid) {
        self.client_broadcast("on_user_delete", user_id.to_string());
    }

    pub fn emit_session_delete(&self, session_id: Uuid) {
        let hub = self.clone();
        let session = session_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            hub.client_send("on_session_delete", session.clone(), session);
        });
    }

    pub fn emit_asset_hidden(&self, user_id: Uuid, asset_id: Uuid) {
        self.client_send("on_asset_hidden", user_id.to_string(), asset_id.to_string());
    }

    pub fn emit_asset_update(&self, user_id: Uuid, asset: AssetResponse) {
        self.client_send("on_asset_update", user_id.to_string(), asset);
    }

    pub fn emit_upload_success(&self, user_id: Uuid, asset: AssetResponse) {
        self.client_send("on_upload_success", user_id.to_string(), asset);
    }

    pub fn emit_asset_upload_ready(&self, user_id: Uuid, payload: AssetUploadReadyV2) {
        self.client_send("AssetUploadReadyV2", user_id.to_string(), payload);
    }

    pub fn emit_asset_edit_ready(&self, user_id: Uuid, payload: AssetEditReadyV2) {
        self.client_send("AssetEditReadyV2", user_id.to_string(), payload);
    }

    pub fn emit_person_thumbnail(&self, user_id: Uuid, person_id: Uuid) {
        self.client_send(
            "on_person_thumbnail",
            user_id.to_string(),
            person_id.to_string(),
        );
    }

    pub fn emit_notification(&self, user_id: Uuid, notification: NotificationResponse) {
        self.client_send("on_notification", user_id.to_string(), notification);
    }
}

async fn auth_middleware<A: Adapter>(
    socket: SocketRef<A>,
    State(app_state): State<WebsocketAppState>,
    HttpExtension(headers): HttpExtension<HeaderMap>,
) -> Result<(), &'static str> {
    let tokens = extract_auth_tokens(&headers, &Default::default());
    let shared_link_tokens = get_shared_link_tokens(&headers);

    match app_state
        .auth
        .authenticate(&tokens, WS_PATH, &shared_link_tokens)
        .await
    {
        Ok(auth_dto) => {
            socket.extensions.insert(auth_dto);
            Ok(())
        }
        Err(_) => Err("unauthorized"),
    }
}

async fn on_connect<A: Adapter>(
    socket: SocketRef<A>,
    Extension(auth): Extension<AuthDto>,
    State(app_state): State<WebsocketAppState>,
    io: SocketIo<A>,
) {
    let user_room = auth.user.id.to_string();

    let _ = socket.join(user_room.clone());
    if let Some(session) = &auth.session {
        let _ = socket.join(session.id.clone());
    }

    let version = ServerService::version();
    let _ = io
        .to(user_room.clone())
        .emit("on_server_version", &version)
        .await;

    let pool = app_state.pool.clone();
    let user_id = user_room.clone();
    if let Err(err) =
        crate::service::version_check::on_websocket_connect(&pool, |event, payload| {
            let io = io.clone();
            let user_id = user_id.clone();
            tokio::spawn(async move {
                let _ = io.to(user_id).emit(event, &payload).await;
            });
        })
        .await
    {
        eprintln!("websocket version check notification failed: {err}");
    }
}
