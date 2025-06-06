use bb8::Pool;
use crate::dtos::env_dto::EnvDto;
use bb8_redis::RedisConnectionManager;
use rbatis::RBatis;

pub const OFFSET_TIME: i32 = 8 * 60 * 60;

#[derive(Clone)]
pub struct AppState {
    pub rb: RBatis,
    pub redis_pool: Pool<RedisConnectionManager>,
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
        let pool = Pool::builder().build(manager).await.unwrap();

        let batis = RBatis::new();
        let string = format!(
            "postgres://{}:{}@{}:{}/{}",
            settings.db_username,
            settings.db_password,
            settings.db_url,
            settings.db_port,
            settings.db_database_name,
        );
        batis.link(rbdc_pg::driver::PgDriver {}, string.as_str()).await.unwrap();
        AppState {
            rb: batis,
            redis_pool: pool,
        }
    }
}
