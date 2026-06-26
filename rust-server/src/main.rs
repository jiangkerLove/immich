use axum::middleware;
use axum::Router;
use config::{Case, Config};
use dotenv::dotenv;

use rust_server::app_state::AppState;
use rust_server::middleware::{auth, cors, user_agent};
use rust_server::models::dto::env::EnvDto;
use rust_server::routes;

#[tokio::main]
async fn main() {
    dotenv().ok();

    let settings: EnvDto = Config::builder()
        .add_source(config::Environment::with_convert_case(Case::UpperSnake).try_parsing(true))
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();

    let port = settings.immich_port.unwrap_or(2283);

    let (app_state, websocket_layer) = AppState::new(settings).await;

    let protected_routes = routes::protected_router().route_layer(middleware::from_fn_with_state(
        app_state.clone(),
        auth::require_auth,
    ));

    let app = Router::new()
        .merge(routes::public_router())
        .merge(protected_routes)
        .route_layer(middleware::from_fn(user_agent::user_agent))
        .route_layer(middleware::from_fn(cors::cors))
        .layer(websocket_layer)
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();

    println!("rust-server listening on 0.0.0.0:{port}");
    axum::serve(listener, app).await.unwrap();
}
