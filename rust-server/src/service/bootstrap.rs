use axum::middleware;
use axum::Router;
use config::{Case, Config};

use crate::app_state::AppState;
use crate::middleware::{cors, telemetry, user_agent};
use crate::models::dto::env::EnvDto;
use crate::routes::{maintenance_worker, public_router, protected_router, static_web};
use crate::service::lifecycle;
use crate::utils::telemetry as telemetry_util;
use crate::utils::workers;
use crate::utils::host_metrics;
use crate::utils::storage::StoragePaths;

pub enum ServerMode {
    Api,
    Maintenance,
}

pub fn load_env() -> EnvDto {
    dotenv::dotenv().ok();
    Config::builder()
        .add_source(config::Environment::with_convert_case(Case::UpperSnake).try_parsing(true))
        .build()
        .expect("failed to load config")
        .try_deserialize()
        .expect("invalid environment")
}

pub async fn run(mode: ServerMode) {
    let settings = load_env();
    let telemetry = telemetry_util::init(&settings);
    if telemetry_util::metrics_enabled() {
        let port = if telemetry_util::api_metrics_enabled() {
            telemetry.api_port
        } else {
            telemetry.microservices_port
        };
        telemetry_util::spawn_prometheus_exporter(port);
        println!("prometheus metrics listening on 0.0.0.0:{port}");
    }

    if telemetry_util::host_metrics_enabled() {
        let media = StoragePaths::new(
            settings
                .immich_media_location
                .as_ref()
                .or(settings.upload_location.as_ref())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("./library")),
        );
        host_metrics::spawn_collector(media.media_location().to_path_buf());
    }

    let port = settings.immich_port.unwrap_or(2283);
    let web_root = static_web::resolve_web_root(&settings);

    let (app_state, websocket_layer, router) = match mode {
        ServerMode::Api => {
            let (state, layer) = AppState::new(settings).await;
            let api = Router::new()
                .merge(public_router())
                .merge(protected_router())
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::middleware::auth::require_auth,
                ));
            let app = if let Some(root) = web_root.as_ref() {
                Router::new()
                    .merge(api)
                    .merge(static_web::fallback_router(root))
            } else {
                api
            };
            (state, layer, app)
        }
        ServerMode::Maintenance => {
            let (state, layer) = AppState::new_maintenance(settings).await;
            let app = maintenance_worker::router(web_root.as_deref());
            (state, layer, app)
        }
    };

    let app = router
        .route_layer(middleware::from_fn(telemetry::track_http_metrics))
        .route_layer(middleware::from_fn(user_agent::user_agent))
        .route_layer(middleware::from_fn(cors::cors))
        .layer(websocket_layer)
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("failed to bind port");

    match mode {
        ServerMode::Api => println!("rust-server listening on 0.0.0.0:{port}"),
        ServerMode::Maintenance => println!("rust-server maintenance worker on 0.0.0.0:{port}"),
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    lifecycle::on_shutdown().await;
    println!("rust-server stopped");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    println!("Shutdown signal received, stopping HTTP server...");
}

pub fn resolve_server_mode(settings: &EnvDto, argv_mode: Option<&str>) -> ServerMode {
    if argv_mode == Some("maintenance") {
        return ServerMode::Maintenance;
    }
    if workers::is_maintenance_worker(settings) {
        ServerMode::Maintenance
    } else {
        ServerMode::Api
    }
}
