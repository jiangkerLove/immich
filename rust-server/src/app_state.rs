use std::path::PathBuf;

use bb8::Pool as RedisPool;
use redis::Client as RedisClient;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool as SqlPool, Postgres};

use crate::models::dto::env::EnvDto;
use crate::service::shared_link::SharedLinkService;
use crate::service::asset::AssetService;
use crate::service::asset_file::AssetFileService;
use crate::service::asset_media::AssetMediaService;
use crate::service::cluster_group::ClusterGroupService;
use crate::service::oauth::OAuthService;
use crate::service::search::SearchService;
use crate::service::system_metadata::SystemMetadataService;
use crate::service::trash::TrashService;
use crate::service::timeline::TimelineService;
use crate::service::job::JobService;
use crate::service::{
    album::AlbumService, api_key::ApiKeyService, auth::AuthService, session::SessionService,
    tag::TagService, user::UserService, partner::PartnerService, stack::StackService,
    person::PersonService, activity::ActivityService, map::MapService,
    download::DownloadService,
    view::ViewService,
    user_admin::UserAdminService,
    duplicate::DuplicateService,
    system_config::SystemConfigService,
    maintenance::MaintenanceService,
    maintenance_worker::MaintenanceWorkerRuntime,
    hls::HlsService,
    queue::QueueService,
    library::LibraryService,
    integrity::IntegrityService,
    database_backup::DatabaseBackupService,
    plugin::PluginService,
    workflow::WorkflowService,
    auth_admin::AuthAdminService,
};
use crate::service::memory::MemoryService;
use crate::service::notification::NotificationService;
use crate::service::sync::SyncService;
use crate::service::server::{ServerBuildConfig, ServerService};
use crate::service::websocket::{AppSocketIoLayer, WebSocketHub};
use crate::service::websocket_jobs::WebSocketJobListener;
use crate::service::workers::{self, WorkerContext};
use crate::utils::storage::StoragePaths;

#[derive(Clone)]
pub struct AppState {
    pub sql_pool: SqlPool<Postgres>,
    pub redis_pool: RedisPool<RedisClient>,
    pub services: Services,
    pub storage: StoragePaths,
    pub websocket: WebSocketHub,
    pub maintenance_worker: Option<MaintenanceWorkerRuntime>,
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
    pub asset_file: AssetFileService,
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
    pub sync: SyncService,
    pub partner: PartnerService,
    pub stack: StackService,
    pub person: PersonService,
    pub cluster_group: ClusterGroupService,
    pub activity: ActivityService,
    pub map: MapService,
    pub download: DownloadService,
    pub view: ViewService,
    pub user_admin: UserAdminService,
    pub duplicate: DuplicateService,
    pub system_config: SystemConfigService,
    pub maintenance: MaintenanceService,
    pub hls: HlsService,
    pub queue: QueueService,
    pub library: LibraryService,
    pub integrity: IntegrityService,
    pub database_backup: DatabaseBackupService,
    pub plugin: PluginService,
    pub workflow: WorkflowService,
    pub auth_admin: AuthAdminService,
}

impl Services {
    pub fn new(
        pool: SqlPool<Postgres>,
        redis_url: String,
        storage: StoragePaths,
        env: &EnvDto,
        websocket: WebSocketHub,
        hls_engine: std::sync::Arc<crate::service::transcoding::HlsEngine>,
    ) -> Self {
        let jobs = JobService::new(redis_url.clone());
        let library_path = storage.media_location().to_path_buf();
        let albums = AlbumService::new(pool.clone(), jobs.clone());
        Self {
            auth: AuthService::with_websocket(pool.clone(), websocket.clone()),
            user: UserService::new(pool.clone(), storage.clone()),
            server: ServerService::new(
                pool.clone(),
                ServerBuildConfig::from_env(env),
                library_path.clone(),
                env.immich_config_file.is_some(),
                env.immich_allow_setup.unwrap_or(true),
            ),
            session: SessionService::new(pool.clone(), websocket.clone()),
            api_key: ApiKeyService::new(pool.clone()),
            album: albums.clone(),
            tag: TagService::new(pool.clone(), jobs.clone()),
            asset: AssetService::new(pool.clone(), jobs.clone(), websocket.clone()),
            asset_file: AssetFileService::new(pool.clone(), jobs.clone()),
            shared_link: SharedLinkService::new(pool.clone(), albums),
            asset_media: AssetMediaService::new(pool.clone(), storage.clone(), jobs.clone()),
            oauth: OAuthService::new(pool.clone(), websocket.clone()),
            timeline: TimelineService::new(pool.clone()),
            trash: TrashService::new(pool.clone(), jobs.clone(), websocket.clone()),
            search: SearchService::new(pool.clone()),
            system_metadata: SystemMetadataService::new(pool.clone()),
            job: jobs.clone(),
            memory: MemoryService::new(pool.clone()),
            notification: NotificationService::new(pool.clone(), websocket.clone()),
            sync: SyncService::new(pool.clone()),
            partner: PartnerService::new(pool.clone()),
            stack: StackService::new(pool.clone(), websocket.clone()),
            person: PersonService::new(pool.clone(), jobs.clone()),
            cluster_group: ClusterGroupService::new(pool.clone(), jobs.clone()),
            activity: ActivityService::new(pool.clone()),
            map: MapService::new(pool.clone()),
            download: DownloadService::new(pool.clone()),
            view: ViewService::new(pool.clone()),
            user_admin: UserAdminService::new(pool.clone(), websocket.clone(), jobs.clone()),
            duplicate: DuplicateService::new(pool.clone(), jobs.clone(), websocket.clone()),
            system_config: SystemConfigService::new(pool.clone(), redis_url.clone(), websocket.clone()),
            maintenance: MaintenanceService::new(pool.clone(), storage.clone(), websocket.clone()),
            hls: HlsService::new(pool.clone(), storage.clone(), hls_engine),
            queue: QueueService::new(jobs.clone()),
            library: LibraryService::new(pool.clone(), jobs.clone(), library_path),
            integrity: IntegrityService::new(pool.clone(), websocket.clone()),
            database_backup: DatabaseBackupService::new(storage.clone()),
            plugin: PluginService::new(pool.clone()),
            workflow: WorkflowService::new(pool.clone()),
            auth_admin: AuthAdminService::new(pool.clone()),
        }
    }
}

fn is_none_or_empty(opt: &Option<String>) -> bool {
    opt.as_deref().map_or(true, |s| s.is_empty())
}

fn resolve_media_location(settings: &EnvDto) -> PathBuf {
    crate::service::storage_bootstrap::detect_media_location(settings)
}

impl AppState {
    pub async fn new(settings: EnvDto) -> (Self, AppSocketIoLayer) {
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
        let redis_client = RedisClient::open(redis_url.clone()).expect("invalid redis url");
        let redis_pool = bb8::Pool::builder()
            .build(redis_client)
            .await
            .expect("failed to connect to redis");

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

        crate::models::db::advisory_lock::wait_for_free_maintenance_lock(&sql_pool).await;

        if let Err(err) = crate::service::database_bootstrap::on_startup(&sql_pool, &settings).await {
            panic!("Database bootstrap failed: {err}");
        }

        if let Err(err) = crate::service::database_migrations::run(&settings).await {
            panic!("Failed to run database migrations: {err}");
        }

        if !settings.db_skip_migrations.unwrap_or(false) {
            crate::service::database_bootstrap::log_schema_drift(&sql_pool).await;
        }

        let storage = StoragePaths::new(resolve_media_location(&settings));
        if let Err(err) =
            crate::service::storage_bootstrap::on_bootstrap(&sql_pool, &settings, &storage).await
        {
            panic!("Storage bootstrap failed: {err}");
        }

        let auth = AuthService::new(sql_pool.clone());
        let (websocket_layer, websocket) = WebSocketHub::build(auth, &redis_url)
            .await
            .expect("failed to initialize websocket redis adapter");

        WebSocketJobListener::spawn(sql_pool.clone(), redis_url.clone(), websocket.clone());
        crate::service::server_events::spawn_listener(sql_pool.clone(), redis_url.clone());

        let jobs = JobService::new(redis_url.clone());
        workers::spawn_all(WorkerContext {
            pool: sql_pool.clone(),
            redis_url: redis_url.clone(),
            storage: storage.clone(),
            env: settings.clone(),
            websocket: websocket.clone(),
            jobs: jobs.clone(),
        });

        if crate::utils::workers::should_run_microservices_workers(&settings) {
            crate::service::nightly::spawn(sql_pool.clone(), jobs.clone());
            crate::service::integrity_scheduler::spawn(sql_pool.clone(), jobs.clone());
            crate::service::backup_scheduler::spawn(sql_pool.clone(), jobs.clone());
            crate::service::library_scheduler::spawn(sql_pool.clone(), jobs.clone());
            crate::service::version_scheduler::spawn(sql_pool.clone(), jobs.clone());
        }

        let hls_engine = crate::service::transcoding::HlsEngine::spawn(
            sql_pool.clone(),
            storage.clone(),
        );
        crate::service::lifecycle::register_hls_engine(hls_engine.clone());
        crate::service::config_bootstrap::run(&sql_pool, &settings, &jobs).await;

        if crate::utils::telemetry::api_metrics_enabled() {
            match crate::models::db::users::UserDb::count_active(&sql_pool).await {
                Ok(count) => crate::utils::telemetry::set_users_total(count),
                Err(err) => eprintln!("telemetry: failed to load user count: {err}"),
            }
        }

        (
            AppState {
                sql_pool: sql_pool.clone(),
                redis_pool: redis_pool.clone(),
                storage: storage.clone(),
                services: Services::new(
                    sql_pool,
                    redis_url,
                    storage.clone(),
                    &settings,
                    websocket.clone(),
                    hls_engine,
                ),
                websocket,
                maintenance_worker: None,
            },
            websocket_layer,
        )
    }

    pub async fn new_maintenance(settings: EnvDto) -> (Self, AppSocketIoLayer) {
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
        let redis_client = RedisClient::open(redis_url.clone()).expect("invalid redis url");
        let redis_pool = bb8::Pool::builder()
            .build(redis_client)
            .await
            .expect("failed to connect to redis");

        let db_connection_str = format!(
            "postgres://{}:{}@{}:{}/{}",
            settings.db_username,
            settings.db_password,
            settings.db_url,
            settings.db_port,
            settings.db_database_name,
        );

        let sql_pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(3))
            .connect(&db_connection_str)
            .await
            .expect("can't connect to database");

        let storage = StoragePaths::new(resolve_media_location(&settings));
        if let Err(err) =
            crate::service::storage_bootstrap::on_bootstrap(&sql_pool, &settings, &storage).await
        {
            panic!("Storage bootstrap failed: {err}");
        }

        let auth = AuthService::new(sql_pool.clone());
        let (websocket_layer, websocket) = WebSocketHub::build(auth, &redis_url)
            .await
            .expect("failed to initialize websocket redis adapter");

        let hls_engine = crate::service::transcoding::HlsEngine::spawn(sql_pool.clone(), storage.clone());
        crate::service::lifecycle::register_hls_engine(hls_engine.clone());

        let maintenance_worker = MaintenanceWorkerRuntime::spawn(
            sql_pool.clone(),
            storage.clone(),
            settings.clone(),
            websocket.clone(),
        )
        .await;

        (
            Self {
                sql_pool: sql_pool.clone(),
                redis_pool,
                storage: storage.clone(),
                services: Services::new(
                    sql_pool,
                    redis_url,
                    storage,
                    &settings,
                    websocket.clone(),
                    hls_engine,
                ),
                websocket,
                maintenance_worker: Some(maintenance_worker),
            },
            websocket_layer,
        )
    }
}
