use std::path::PathBuf;

use bb8::Pool as RedisPool;
use bb8_redis::RedisConnectionManager;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool as SqlPool, Postgres};

use crate::models::dto::env::EnvDto;
use crate::service::shared_link::SharedLinkService;
use crate::service::asset::AssetService;
use crate::service::asset_media::AssetMediaService;
use crate::service::oauth::OAuthService;
use crate::service::search::SearchService;
use crate::service::system_metadata::SystemMetadataService;
use crate::service::trash::TrashService;
use crate::service::timeline::TimelineService;
use crate::service::job::JobService;
use crate::service::{
    album::AlbumService, api_key::ApiKeyService, auth::AuthService, session::SessionService,
    tag::TagService, user::UserService,
};
use crate::service::memory::MemoryService;
use crate::service::notification::NotificationService;
use crate::service::server::{ServerBuildConfig, ServerService};
use crate::utils::storage::StoragePaths;

#[derive(Clone)]
pub struct AppState {
    pub sql_pool: SqlPool<Postgres>,
    pub redis_pool: RedisPool<RedisConnectionManager>,
    pub services: Services,
    pub storage: StoragePaths,
}

#[derive(Clone)]
pub struct Services {
    pub auth: AuthService,
    pub user: UserService,
    pub server: ServerService,
    pub session: SessionService,
    pub api_key: ApiKeyService,
    pub album: AlbumService,
    pub tag: TagService,
    pub asset: AssetService,
    pub shared_link: SharedLinkService,
    pub asset_media: AssetMediaService,
    pub oauth: OAuthService,
    pub timeline: TimelineService,
    pub trash: TrashService,
    pub search: SearchService,
    pub system_metadata: SystemMetadataService,
    pub job: JobService,
    pub memory: MemoryService,
    pub notification: NotificationService,
}

impl Services {
    pub fn new(
        pool: SqlPool<Postgres>,
        redis_url: String,
        storage: StoragePaths,
        env: &EnvDto,
    ) -> Self {
        let jobs = JobService::new(redis_url);
        let library_path = storage.media_location().to_path_buf();
        Self {
            auth: AuthService::new(pool.clone()),
            user: UserService::new(pool.clone()),
            server: ServerService::new(
                pool.clone(),
                ServerBuildConfig::from_env(env),
                library_path,
            ),
            session: SessionService::new(pool.clone()),
            api_key: ApiKeyService::new(pool.clone()),
            album: AlbumService::new(pool.clone()),
            tag: TagService::new(pool.clone()),
            asset: AssetService::new(pool.clone(), jobs.clone()),
            shared_link: SharedLinkService::new(pool.clone()),
            asset_media: AssetMediaService::new(pool.clone(), storage.clone()),
            oauth: OAuthService::new(pool.clone()),
            timeline: TimelineService::new(pool.clone()),
            trash: TrashService::new(pool.clone(), jobs.clone()),
            search: SearchService::new(pool.clone()),
            system_metadata: SystemMetadataService::new(pool.clone()),
            job: jobs,
            memory: MemoryService::new(pool.clone()),
            notification: NotificationService::new(pool.clone()),
        }
    }
}

fn is_none_or_empty(opt: &Option<String>) -> bool {
    opt.as_deref().map_or(true, |s| s.is_empty())
}

fn resolve_media_location(settings: &EnvDto) -> PathBuf {
    settings
        .immich_media_location
        .as_ref()
        .or(settings.upload_location.as_ref())
        .cloned()
        .unwrap_or_else(|| "./library".to_string())
        .into()
}

impl AppState {
    pub async fn new(settings: EnvDto) -> Self {
        let redis_url = if !is_none_or_empty(&settings.redis_username)
            && !is_none_or_empty(&settings.redis_password)
        {
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
        let manager = RedisConnectionManager::new(redis_url.clone()).unwrap();
        let redis_pool = bb8::Pool::builder().build(manager).await.unwrap();

        let db_connection_str = format!(
            "postgres://{}:{}@{}:{}/{}",
            settings.db_username,
            settings.db_password,
            settings.db_url,
            settings.db_port,
            settings.db_database_name,
        );

        let sql_pool = PgPoolOptions::new()
            .max_connections(20)
            .acquire_timeout(std::time::Duration::from_secs(3))
            .connect(&db_connection_str)
            .await
            .expect("can't connect to database");

        let storage = StoragePaths::new(resolve_media_location(&settings));

        AppState {
            sql_pool: sql_pool.clone(),
            redis_pool: redis_pool.clone(),
            storage: storage.clone(),
            services: Services::new(sql_pool, redis_url, storage, &settings),
        }
    }
}
