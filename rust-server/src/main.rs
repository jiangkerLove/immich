use axum::Router;
use config::{Case, Config};
use dotenv::dotenv;
use rust_server::app_state::AppState;
use rust_server::dtos::env_dto::EnvDto;
use std::sync::Arc;

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

    let app_state = Arc::new(AppState::new(settings).await);
    let app = Router::new()
        // .route_layer(middleware::from_fn_with_state(Arc::clone(&app_state), auth::auth))
        // .route_layer(middleware::from_fn(cors::cors))
        .with_state(app_state);



    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}




