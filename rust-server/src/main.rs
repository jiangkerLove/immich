use axum::routing::post;
use axum::{middleware, Router};
use config::{Case, Config};
use dotenv::dotenv;
use rust_server::app_state::AppState;
use rust_server::controllers::auth::login;
use rust_server::dtos::env_dto::EnvDto;
use rust_server::middleware::user_agent;

#[tokio::main]
async fn main() {
    // 读取 .env 文件
    dotenv().ok();

    let settings: EnvDto = Config::builder()
        .add_source(config::Environment::with_convert_case(Case::UpperSnake).try_parsing(true)) // 解析 ENV 变量
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();

    let app_state = AppState::new(settings).await;
    let app = Router::new()
        .route("/api/auth/login", post(login))
        .route_layer(middleware::from_fn(user_agent::user_agent))
        // .route_layer(middleware::from_fn(cors::cors))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}




