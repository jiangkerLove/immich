use std::time::Duration;
use bb8::Pool as RedisPool;
use crate::dtos::env_dto::EnvDto;
use bb8_redis::RedisConnectionManager;
use sqlx::{Pool as SqlPool, Postgres};
use sqlx::postgres::PgPoolOptions;
use crate::service::auth::AuthService;

pub const OFFSET_TIME: i32 = 8 * 60 * 60;

#[derive(Clone)]
pub struct AppState {
    pub sql_pool: SqlPool<Postgres>,
    pub redis_pool: RedisPool<RedisConnectionManager>,
    pub auth_service: AuthService,
}
fn is_none_or_empty(opt: &Option<String>) -> bool {
    opt.as_deref().map_or(true, |s| s.is_empty())
}
impl AppState {
    pub async fn new(settings: EnvDto) -> AppState {
        let redis_url = if !is_none_or_empty(&settings.redis_username) && !is_none_or_empty(&settings.redis_password) {
            format!(
                "redis://{}:{}@{}:{}",
                settings.redis_username.as_ref().unwrap(),
                settings.redis_password.as_ref().unwrap(),
                settings.redis_hostname,
                settings.redis_port
            )
        } else {
            format!("redis://{}:{}", settings.redis_hostname, settings.redis_port)
        };
        let manager = RedisConnectionManager::new(redis_url).unwrap();
        let redis_pool = RedisPool::builder().build(manager).await.unwrap();

        let db_connection_str = format!(
            "postgres://{}:{}@{}:{}/{}",
            settings.db_username,
            settings.db_password,
            settings.db_url,
            settings.db_port,
            settings.db_database_name,
        );

        // set up connection pool
        let sql_pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .connect(&db_connection_str.as_str())
            .await
            .expect("can't connect to database");
        AppState {
            sql_pool: sql_pool.clone(),
            redis_pool,
            auth_service: AuthService::new(sql_pool),
        }
    }
}
