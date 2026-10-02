use chrono::Utc;
use sqlx::PgPool;

use crate::constants::{LOGIN_DUMMY_HASH, LOGIN_URL};
use crate::ext::bcrypt::{BcryptCompare, hash_bcrypt};
use crate::models::db::api_key::ApiKeyRow;
use crate::models::db::auth_permission::Permission;
use crate::models::db::sessions::{AuthSession, NewSession, SessionPO};
use crate::models::db::shared_links;
use crate::models::db::user_metadata::UserMetadataPO;
use crate::models::db::users::{NewUserDb, UserDb, map_user_admin_with_license};
use crate::models::dto::auth::AuthDto;
use crate::models::request::auth::{
    ChangePasswordReq, LoginCredentialReq, LoginReq, PinCodeChangeReq, PinCodeResetReq,
    PinCodeSetupReq, SessionUnlockReq, SignUpReq,
};
use crate::models::response::auth::{
    AuthStatusResp, LoginResp, LogoutResp, ValidateAccessTokenResp,
};
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::service::websocket::WebSocketHub;
use crate::utils::checksum::decode_share_key;
use crate::utils::crypto::{hash_sha256, random_bytes_as_text};
use crate::utils::headers::AuthTokens;
use crate::utils::permission::require_permission;

#[derive(Clone)]
pub struct AuthService {
    db_pool: PgPool,
    websocket: Option<WebSocketHub>,
}

impl AuthService {
    pub fn new(db_pool: PgPool) -> Self {
        Self {
            db_pool,
            websocket: None,
        }
    }

    pub fn with_websocket(db_pool: PgPool, websocket: WebSocketHub) -> Self {
        Self {
            db_pool,
            websocket: Some(websocket),
        }
    }

    pub async fn login(
        &self,
        login_credential: &LoginCredentialReq,
        login_details: &LoginReq,
    ) -> Result<LoginResp, ErrorResp> {
        if !crate::models::db::system_metadata::password_login_enabled(&self.db_pool)
            .await
            .map_err(ErrorResp::from)?
        {
            return Err(ErrorResp::Unauthorized(
                "Password login has been disabled".to_string(),
            ));
        }

        let email = login_credential.email.clone();
        let user_option = UserDb::select_full_by_email(&self.db_pool, &email)
            .await
            .map_err(ErrorResp::from)?;

        let password_hash = user_option
            .as_ref()
            .map(|user| user.password.as_str())
            .unwrap_or(LOGIN_DUMMY_HASH);

        let authenticated = login_credential
            .password
            .as_str()
            .compare_bcrypt(password_hash)
            .is_ok_and(|ok| ok);

        if user_option.is_none()
            || user_option
                .as_ref()
                .is_some_and(|user| user.password.is_empty())
            || !authenticated
        {
            return Err(ErrorResp::Unauthorized(
                "Incorrect email or password".to_string(),
            ));
        }

        self.create_login_response(user_option.unwrap(), login_details)
            .await
    }

    pub async fn admin_sign_up(&self, dto: &SignUpReq) -> Result<UserAdminResponse, ErrorResp> {
        if !crate::service::config_bootstrap::setup_allowed() {
            return Err(ErrorResp::BadRequest(
                "Admin setup is not available".to_string(),
            ));
        }
        if UserDb::get_admin(&self.db_pool).await?.is_some() {
            return Err(ErrorResp::BadRequest(
                "The server already has an admin".to_string(),
            ));
        }

        let hashed_password =
            hash_bcrypt(&dto.password).map_err(|err| ErrorResp::ServerError(err.to_string()))?;

        let user = UserDb::insert(
            &self.db_pool,
            &NewUserDb {
                email: dto.email.clone(),
                password: hashed_password,
                name: dto.name.clone(),
                is_admin: true,
                storage_label: Some("admin".to_string()),
            },
        )
        .await
        .map_err(|err| {
            if let sqlx::Error::Database(db_err) = &err {
                if db_err.constraint().is_some() {
                    return ErrorResp::BadRequest("Email is not available".to_string());
                }
            }
            ErrorResp::from(err)
        })?;

        crate::utils::telemetry::add_users_total(1);
        Ok(map_user_admin_with_license(&self.db_pool, user).await?)
    }

    pub async fn logout(
        &self,
        auth: &AuthDto,
        auth_type: Option<&str>,
    ) -> Result<LogoutResp, ErrorResp> {
        let mut oauth_bearer = None;
        if let Some(session) = &auth.session {
            if let Ok(session_id) = uuid::Uuid::parse_str(&session.id) {
                if let Some(row) = SessionPO::get_by_id(&self.db_pool, &session_id).await? {
                    oauth_bearer = row.oauth_bearer_token;
                }
                SessionPO::delete(&self.db_pool, &session_id).await?;
                if let Some(ws) = &self.websocket {
                    ws.emit_session_delete(session_id);
                }
            }
        }

        let redirect_uri = if auth_type == Some("oauth") {
            oauth_logout_url(&self.db_pool, oauth_bearer.as_deref()).await
        } else {
            LOGIN_URL.to_string()
        };

        Ok(LogoutResp {
            successful: true,
            redirect_uri,
        })
    }

    pub fn validate_access_token() -> ValidateAccessTokenResp {
        ValidateAccessTokenResp { auth_status: true }
    }

    pub async fn get_auth_status(&self, auth: &AuthDto) -> Result<AuthStatusResp, ErrorResp> {
        let user = UserDb::get_for_pin_code(&self.db_pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Unauthorized".to_string()))?;

        let session_po = if let Some(session) = &auth.session {
            if let Ok(session_id) = uuid::Uuid::parse_str(&session.id) {
                SessionPO::get_by_id(&self.db_pool, &session_id).await?
            } else {
                None
            }
        } else {
            None
        };

        Ok(AuthStatusResp {
            pin_code: user.pin_code.is_some(),
            password: !user.password.is_empty(),
            is_elevated: auth
                .session
                .as_ref()
                .is_some_and(|session| session.has_elevated_permission),
            expires_at: session_po
                .as_ref()
                .and_then(|session| session.expires_at)
                .map(|value| value.to_rfc3339()),
            pin_expires_at: session_po
                .and_then(|session| session.pin_expires_at)
                .map(|value| value.to_rfc3339()),
        })
    }

    pub async fn change_password(
        &self,
        auth: &AuthDto,
        dto: &ChangePasswordReq,
    ) -> Result<UserAdminResponse, ErrorResp> {
        require_permission(auth, Permission::AuthChangePassword)?;

        if dto.new_password.len() < 8 {
            return Err(ErrorResp::BadRequest(
                "New password must be at least 8 characters".to_string(),
            ));
        }

        let user = UserDb::get_for_change_password(&self.db_pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Authentication required".to_string()))?;

        if !validate_secret(&dto.password, Some(&user.password)) {
            return Err(ErrorResp::BadRequest("Wrong password".to_string()));
        }

        let hashed_password = hash_bcrypt(&dto.new_password)
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?;

        let updated =
            UserDb::update_password(&self.db_pool, &auth.user.id, &hashed_password).await?;

        // Official `onAuthChangePassword` always drops other sessions.
        let _ = dto.invalidate_sessions;
        let current_session_id = auth
            .session
            .as_ref()
            .and_then(|session| uuid::Uuid::parse_str(&session.id).ok());
        SessionPO::invalidate_all_except(&self.db_pool, &auth.user.id, current_session_id.as_ref())
            .await?;

        Ok(map_user_admin_with_license(&self.db_pool, updated).await?)
    }

    pub async fn setup_pin_code(
        &self,
        auth: &AuthDto,
        dto: &PinCodeSetupReq,
    ) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::PinCodeCreate)?;

        if !is_valid_pin_code(&dto.pin_code) {
            return Err(ErrorResp::BadRequest("Invalid PIN code".to_string()));
        }

        let user = UserDb::get_for_pin_code(&self.db_pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Unauthorized".to_string()))?;

        if user.pin_code.is_some() {
            return Err(ErrorResp::BadRequest(
                "User already has a PIN code".to_string(),
            ));
        }

        let hashed =
            hash_bcrypt(&dto.pin_code).map_err(|err| ErrorResp::ServerError(err.to_string()))?;
        UserDb::update_pin_code(&self.db_pool, &auth.user.id, Some(&hashed)).await?;
        Ok(())
    }

    pub async fn change_pin_code(
        &self,
        auth: &AuthDto,
        dto: &PinCodeChangeReq,
    ) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::PinCodeUpdate)?;

        if !is_valid_pin_code(&dto.new_pin_code) {
            return Err(ErrorResp::BadRequest("Invalid PIN code".to_string()));
        }

        let user = UserDb::get_for_pin_code(&self.db_pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Authentication required".to_string()))?;

        validate_pin_code_auth(&user, dto.pin_code.as_deref(), dto.password.as_deref())?;

        let hashed = hash_bcrypt(&dto.new_pin_code)
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?;
        UserDb::update_pin_code(&self.db_pool, &auth.user.id, Some(&hashed)).await?;
        Ok(())
    }

    pub async fn reset_pin_code(
        &self,
        auth: &AuthDto,
        dto: &PinCodeResetReq,
    ) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::PinCodeDelete)?;

        let user = UserDb::get_for_pin_code(&self.db_pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Authentication required".to_string()))?;

        validate_pin_code_auth(&user, dto.pin_code.as_deref(), dto.password.as_deref())?;

        UserDb::update_pin_code(&self.db_pool, &auth.user.id, None).await?;
        SessionPO::lock_all_for_user(&self.db_pool, &auth.user.id).await?;
        Ok(())
    }

    pub async fn unlock_session(
        &self,
        auth: &AuthDto,
        dto: &SessionUnlockReq,
    ) -> Result<(), ErrorResp> {
        let session = auth.session.as_ref().ok_or_else(|| {
            ErrorResp::BadRequest("This endpoint can only be used with a session token".to_string())
        })?;

        let user = UserDb::get_for_pin_code(&self.db_pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Authentication required".to_string()))?;

        let pin_code = dto.pin_code.as_deref().ok_or_else(|| {
            ErrorResp::BadRequest("Either password or pinCode is required".to_string())
        })?;

        if user.pin_code.is_none() {
            return Err(ErrorResp::BadRequest(
                "User does not have a PIN code".to_string(),
            ));
        }

        if !validate_secret(pin_code, user.pin_code.as_deref()) {
            return Err(ErrorResp::BadRequest("Wrong PIN code".to_string()));
        }

        let session_id = uuid::Uuid::parse_str(&session.id)
            .map_err(|_| ErrorResp::BadRequest("Invalid session".to_string()))?;

        SessionPO::update_pin_expires_at(
            &self.db_pool,
            &session_id,
            Some(Utc::now() + chrono::Duration::minutes(15)),
        )
        .await?;

        Ok(())
    }

    pub async fn lock_session(&self, auth: &AuthDto) -> Result<(), ErrorResp> {
        let session = auth.session.as_ref().ok_or_else(|| {
            ErrorResp::BadRequest("This endpoint can only be used with a session token".to_string())
        })?;

        let session_id = uuid::Uuid::parse_str(&session.id)
            .map_err(|_| ErrorResp::BadRequest("Invalid session".to_string()))?;

        SessionPO::update_pin_expires_at(&self.db_pool, &session_id, None).await?;
        Ok(())
    }

    pub async fn authenticate(
        &self,
        tokens: &AuthTokens,
        path: &str,
        shared_link_tokens: &[String],
        client: &crate::models::request::auth::LoginReq,
    ) -> Result<AuthDto, ErrorResp> {
        if let Some(key) = &tokens.share_key {
            return self
                .validate_shared_link_key(key, path, shared_link_tokens)
                .await;
        }

        if let Some(slug) = &tokens.share_slug {
            return self
                .validate_shared_link_slug(slug, path, shared_link_tokens)
                .await;
        }

        if let Some(session) = &tokens.session {
            return self.validate_session(session, client).await;
        }

        if let Some(api_key) = &tokens.api_key {
            return self.validate_api_key(api_key).await;
        }

        Err(ErrorResp::Unauthorized(
            "Authentication required".to_string(),
        ))
    }

    async fn validate_api_key(&self, key: &str) -> Result<AuthDto, ErrorResp> {
        let hashed = hash_sha256(key);
        let api_key = ApiKeyRow::get_by_key(&self.db_pool, &hashed)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Invalid API key".to_string()))?;

        let (user, api_key) = api_key.into_auth();
        Ok(AuthDto {
            user,
            api_key: Some(api_key),
            session: None,
            shared_link: None,
        })
    }

    pub async fn validate_shared_link_key(
        &self,
        key: &str,
        _path: &str,
        _shared_link_tokens: &[String],
    ) -> Result<AuthDto, ErrorResp> {
        let bytes = decode_share_key(key)
            .map_err(|_| ErrorResp::Unauthorized("Invalid share key".to_string()))?;
        let result = shared_links::get_by_key(&self.db_pool, &bytes)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Invalid share key".to_string()))?;
        let (user, shared_link) = result;
        Ok(AuthDto {
            user,
            api_key: None,
            session: None,
            shared_link: Some(shared_link),
        })
    }

    pub async fn validate_shared_link_slug(
        &self,
        slug: &str,
        _path: &str,
        _shared_link_tokens: &[String],
    ) -> Result<AuthDto, ErrorResp> {
        let result = shared_links::get_by_slug(&self.db_pool, slug)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Invalid share slug".to_string()))?;
        let (user, shared_link) = result;
        Ok(AuthDto {
            user,
            api_key: None,
            session: None,
            shared_link: Some(shared_link),
        })
    }

    async fn validate_session(
        &self,
        token_value: &str,
        client: &crate::models::request::auth::LoginReq,
    ) -> Result<AuthDto, ErrorResp> {
        let token = hash_sha256(token_value);
        let session = SessionPO::query_by_token(&self.db_pool, &token)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Invalid user token".to_string()))?;

        let user = AuthUserDb::select_user_by_id(&self.db_pool, &session.user_id)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Invalid user token".to_string()))?;

        let now = Utc::now();
        let stale = now.signed_duration_since(session.updated_at).num_hours() > 1
            || client.app_version.as_deref() != session.app_version.as_deref();
        if stale {
            SessionPO::update_device(
                &self.db_pool,
                &session.id,
                &client.device_os,
                &client.device_type,
                client.app_version.as_deref(),
            )
            .await?;
        }

        let mut has_elevated_permission = false;
        if let Some(pin_expires_at) = session.pin_expires_at {
            has_elevated_permission = pin_expires_at > now;
            if has_elevated_permission && now + chrono::Duration::minutes(5) > pin_expires_at {
                SessionPO::update_pin_expires_at(
                    &self.db_pool,
                    &session.id,
                    Some(now + chrono::Duration::minutes(5)),
                )
                .await?;
            }
        }

        Ok(AuthDto {
            user,
            api_key: None,
            session: Some(AuthSession {
                id: session.id.to_string(),
                has_elevated_permission,
            }),
            shared_link: None,
        })
    }

    async fn create_login_response(
        &self,
        user_po: UserDb,
        _login_details: &LoginReq,
    ) -> Result<LoginResp, ErrorResp> {
        let token = random_bytes_as_text(32);
        let hash_token = hash_sha256(&token);
        let is_onboarded = UserMetadataPO::is_onboarded(&self.db_pool, &user_po.id).await?;

        let session = NewSession {
            token: hash_token,
            device_os: _login_details.device_os.clone(),
            device_type: _login_details.device_type.clone(),
            app_version: _login_details.app_version.clone(),
            user_id: user_po.id,
            oauth_sid: None,
            oauth_bearer_token: None,
        };

        session.insert(&self.db_pool).await?;

        Ok(LoginResp {
            access_token: token,
            user_id: user_po.id,
            user_email: user_po.email,
            name: user_po.name,
            is_admin: user_po.is_admin,
            profile_image_path: user_po.profile_image_path,
            should_change_password: user_po.should_change_password,
            is_onboarded,
        })
    }
}

use crate::models::db::users::AuthUserDb;

async fn oauth_logout_url(pool: &PgPool, id_token: Option<&str>) -> String {
    let Ok(config) = crate::utils::system_config::get_merged(pool).await else {
        return LOGIN_URL.to_string();
    };
    let enabled = crate::utils::system_config::json_bool(&config, &["oauth", "enabled"], false);
    let mut endpoint =
        crate::utils::system_config::json_str(&config, &["oauth", "endSessionEndpoint"], "");
    if endpoint.is_empty() {
        let issuer = crate::utils::system_config::json_str(&config, &["oauth", "issuerUrl"], "");
        if let Some(discovered) =
            crate::service::oauth::discover_end_session_endpoint(&issuer).await
        {
            endpoint = discovered;
        }
    }
    if !enabled || endpoint.is_empty() {
        return LOGIN_URL.to_string();
    }
    let Ok(mut url) = url::Url::parse(&endpoint) else {
        return LOGIN_URL.to_string();
    };
    if let Some(token) = id_token.filter(|value| !value.is_empty()) {
        url.query_pairs_mut().append_pair("id_token_hint", token);
    }
    url.to_string()
}

pub(crate) fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn is_valid_pin_code(pin_code: &str) -> bool {
    pin_code.len() == 6 && pin_code.chars().all(|c| c.is_ascii_digit())
}

fn validate_secret(input: &str, existing_hash: Option<&str>) -> bool {
    match existing_hash {
        Some(hash) if !hash.is_empty() => input.compare_bcrypt(hash).is_ok_and(|ok| ok),
        _ => false,
    }
}

fn validate_pin_code_auth(
    user: &crate::models::db::users::UserPinAuthDb,
    pin_code: Option<&str>,
    password: Option<&str>,
) -> Result<(), ErrorResp> {
    if user.pin_code.is_none() {
        return Err(ErrorResp::BadRequest(
            "User does not have a PIN code".to_string(),
        ));
    }

    if let Some(password) = password {
        if !validate_secret(password, Some(&user.password)) {
            return Err(ErrorResp::BadRequest("Wrong password".to_string()));
        }
        Ok(())
    } else if let Some(pin_code) = pin_code {
        if !validate_secret(pin_code, user.pin_code.as_deref()) {
            return Err(ErrorResp::BadRequest("Wrong PIN code".to_string()));
        }
        Ok(())
    } else {
        Err(ErrorResp::BadRequest(
            "Either password or pinCode is required".to_string(),
        ))
    }
}
