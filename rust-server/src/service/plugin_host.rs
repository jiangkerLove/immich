use std::sync::Arc;

use extism::{CurrentPlugin, Error, Function, UserData, Val, ValType};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::sessions::AuthSession;
use crate::models::db::users::AuthUserDb;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::album::{
    AlbumService, AlbumsAddAssetsReq, BulkIdsReq, CreateAlbumReq, GetAlbumsQuery,
};
use crate::service::job::JobService;

const HOST_NAMESPACE: &str = "extism:host/user";

#[derive(Clone)]
pub struct HostContext {
    pub pool: PgPool,
    pub jobs: JobService,
    pub jwt_secret: String,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct HostCallInput {
    authToken: String,
    args: Value,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct WorkflowAuthClaims {
    userId: String,
}

struct HostFnState {
    context: Arc<HostContext>,
    name: &'static str,
    stubs: bool,
}

impl HostContext {
    pub fn host_functions(context: Arc<Self>, stubs: bool) -> Vec<Function> {
        const NAMES: [&str; 4] = [
            "searchAlbums",
            "createAlbum",
            "addAssetsToAlbum",
            "addAssetsToAlbums",
        ];

        NAMES
            .into_iter()
            .map(|name| {
                Function::new(
                    name,
                    [ValType::I64],
                    [ValType::I64],
                    UserData::new(HostFnState {
                        context: context.clone(),
                        name,
                        stubs,
                    }),
                    host_function,
                )
                .with_namespace(HOST_NAMESPACE)
            })
            .collect()
    }

    fn album_service(&self) -> AlbumService {
        AlbumService::new(self.pool.clone(), self.jobs.clone())
    }

    async fn auth_from_token(&self, token: &str) -> Result<AuthDto, String> {
        if token.is_empty() {
            return Err("authToken is required".to_string());
        }

        let claims = decode::<WorkflowAuthClaims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )
        .map_err(|err| err.to_string())?
        .claims;

        if claims.userId.is_empty() {
            return Err("Invalid token: missing userId".to_string());
        }

        let user_id = Uuid::parse_str(&claims.userId).map_err(|err| err.to_string())?;

        Ok(AuthDto {
            user: AuthUserDb {
                id: user_id,
                is_admin: false,
                name: String::new(),
                email: String::new(),
                quota_usage_in_bytes: 0,
                quota_size_in_bytes: None,
            },
            api_key: None,
            session: Some(AuthSession {
                id: "workflow".to_string(),
                has_elevated_permission: true,
            }),
            shared_link: None,
        })
    }

    async fn dispatch(&self, name: &str, input: HostCallInput) -> Value {
        match name {
            "searchAlbums" => self.handle_search_albums(input).await,
            "createAlbum" => self.handle_create_album(input).await,
            "addAssetsToAlbum" => self.handle_add_assets_to_album(input).await,
            "addAssetsToAlbums" => self.handle_add_assets_to_albums(input).await,
            other => host_error(400, format!("Unknown host function: {other}")),
        }
    }

    async fn handle_search_albums(&self, input: HostCallInput) -> Value {
        let auth = match self.auth_from_token(&input.authToken).await {
            Ok(auth) => auth,
            Err(message) => return host_error(401, message),
        };
        let query: GetAlbumsQuery = match parse_args(input.args, 0) {
            Ok(value) => value,
            Err(value) => return value,
        };

        match self.album_service().get_all(&auth, &query).await {
            Ok(albums) => host_success(json!(albums)),
            Err(err) => map_error(err),
        }
    }

    async fn handle_create_album(&self, input: HostCallInput) -> Value {
        let auth = match self.auth_from_token(&input.authToken).await {
            Ok(auth) => auth,
            Err(message) => return host_error(401, message),
        };
        let dto: CreateAlbumReq = match parse_args(input.args, 0) {
            Ok(value) => value,
            Err(value) => return value,
        };

        match self.album_service().create(&auth, &dto).await {
            Ok(album) => host_success(json!(album)),
            Err(err) => map_error(err),
        }
    }

    async fn handle_add_assets_to_album(&self, input: HostCallInput) -> Value {
        let auth = match self.auth_from_token(&input.authToken).await {
            Ok(auth) => auth,
            Err(message) => return host_error(401, message),
        };
        let album_id: Uuid = match parse_args(input.args.clone(), 0) {
            Ok(value) => value,
            Err(value) => return value,
        };
        let dto: BulkIdsReq = match parse_args(input.args, 1) {
            Ok(value) => value,
            Err(value) => return value,
        };

        match self.album_service().add_assets(&auth, &album_id, &dto).await {
            Ok(results) => host_success(json!(results)),
            Err(err) => map_error(err),
        }
    }

    async fn handle_add_assets_to_albums(&self, input: HostCallInput) -> Value {
        let auth = match self.auth_from_token(&input.authToken).await {
            Ok(auth) => auth,
            Err(message) => return host_error(401, message),
        };
        let dto: AlbumsAddAssetsReq = match parse_args(input.args, 0) {
            Ok(value) => value,
            Err(value) => return value,
        };

        match self.album_service().add_assets_to_albums(&auth, &dto).await {
            Ok(result) => host_success(json!(result)),
            Err(err) => map_error(err),
        }
    }
}

fn host_function(
    plugin: &mut CurrentPlugin,
    params: &[Val],
    results: &mut [Val],
    user_data: UserData<HostFnState>,
) -> Result<(), Error> {
    let host_state = user_data.get()?;
    let state = host_state
        .lock()
        .map_err(|_| Error::msg("host function lock poisoned"))?;
    let stubs = state.stubs;
    let name = state.name;
    let context = state.context.clone();
    drop(state);

    let output = if stubs {
        host_error(
            400,
            "Calling host functions is not allowed without setting methods[].hostFunctions=true in the plugin manifest",
        )
    } else {
        let handle = plugin
            .memory_from_val(&params[0])
            .ok_or_else(|| Error::msg("Called host function without input"))?;
        let input: String = plugin.memory_get(handle)?;
        let parsed: HostCallInput = serde_json::from_str(&input)
            .map_err(|err| Error::msg(format!("Invalid host function input: {err}")))?;

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(context.dispatch(name, parsed))
        })
    };

    plugin.memory_set_val(&mut results[0], serde_json::to_string(&output).unwrap())?;
    Ok(())
}

fn host_error(status: u16, message: impl Into<String>) -> Value {
    json!({
        "success": false,
        "status": status,
        "message": message.into(),
    })
}

fn host_success(response: Value) -> Value {
    json!({
        "success": true,
        "response": response,
    })
}

fn map_error(err: ErrorResp) -> Value {
    let status = match &err {
        ErrorResp::BadRequest(_) | ErrorResp::ReqParamError(_) => 400,
        ErrorResp::Unauthorized(_) => 401,
        ErrorResp::Forbidden(_) => 403,
        ErrorResp::NotFound(_) => 404,
        ErrorResp::NotImplemented(_) => 501,
        ErrorResp::DatabaseError(_) | ErrorResp::ServerError(_) => 500,
    };
    host_error(status, err.to_string())
}

fn parse_args<T: for<'de> Deserialize<'de>>(args: Value, index: usize) -> Result<T, Value> {
    let value = args
        .get(index)
        .cloned()
        .ok_or_else(|| host_error(400, "Missing host function argument"))?;
    serde_json::from_value(value).map_err(|err| host_error(400, err.to_string()))
}
