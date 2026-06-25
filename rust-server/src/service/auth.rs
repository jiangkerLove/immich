use chrono::Utc;
use sqlx::PgPool;

use crate::constants::{LOGIN_DUMMY_HASH, LOGIN_URL};
use crate::ext::bcrypt::{hash_bcrypt, BcryptCompare};
use crate::models::db::api_key::ApiKeyRow;
use crate::models::db::sessions::{AuthSession, NewSession, SessionPO};
use crate::models::db::shared_links;
use crate::models::db::user_metadata::UserMetadataPO;
use crate::models::db::users::{map_user_admin, NewUserDb, UserDb};
use crate::models::dto::auth::AuthDto;
use crate::models::request::auth::{LoginCredentialReq, LoginReq, SignUpReq};
use crate::models::response::auth::{
    AuthStatusResp, LoginResp, LogoutResp, ValidateAccessTokenResp,
};
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::utils::crypto::{hash_sha256, random_bytes_as_text};
use crate::utils::checksum::decode_share_key;
use crate::utils::headers::AuthTokens;

#[derive(Clone)]
pub struct AuthService {
    db_pool: PgPool,
}

impl AuthService {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn login(
        &self,
        login_credential: &LoginCredentialReq,
        login_details: &LoginReq,
    ) -> Result<LoginResp, ErrorResp> {
        let user_option = UserDb::select_full_by_email(&self.db_pool, &login_credential.email)
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
            || user_option.as_ref().is_some_and(|user| user.password.is_empty())
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
        if UserDb::get_admin(&self.db_pool).await?.is_some() {
            return Err(ErrorResp::BadRequest(
                "The server already has an admin".to_string(),
            ));
        }

        let hashed_password = hash_bcrypt(&dto.password)
            .map_err(|err| ErrorResp::ServerError(err.to_string()))?;

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

        Ok(map_user_admin(user))
    }

    pub async fn logout(&self, auth: &AuthDto) -> Result<LogoutResp, ErrorResp> {
        if let Some(session) = &auth.session {
            if let Ok(session_id) = uuid::Uuid::parse_str(&session.id) {
                SessionPO::delete(&self.db_pool, &session_id).await?;
            }
        }

        Ok(LogoutResp {
            successful: true,
            redirect_uri: LOGIN_URL.to_string(),
        })
    }

    pub fn validate_access_token() -> ValidateAccessTokenResp {
        ValidateAccessTokenResp {
            auth_status: true,
        }
    }

    pub async fn get_auth_status(&self, auth: &AuthDto) -> Result<AuthStatusResp, ErrorResp> {
        let user = UserDb::get_for_pin_code(&self.db_pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Authentication required".to_string()))?;

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

    pub async fn authenticate(
        &self,
        tokens: &AuthTokens,
        path: &str,
        shared_link_tokens: &[String],
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
            return self.validate_session(session).await;
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

    async fn validate_shared_link_key(
        &self,
        key: &str,
        path: &str,
        shared_link_tokens: &[String],
    ) -> Result<AuthDto, ErrorResp> {
        let bytes = decode_share_key(key).map_err(|_| ErrorResp::Unauthorized("Invalid share key".to_string()))?;
        let result = shared_links::get_by_key(&self.db_pool, &bytes)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Invalid share key".to_string()))?;
        let (user, shared_link) = result;
        self.require_shared_link_password(&shared_link, path, shared_link_tokens)?;
        Ok(AuthDto {
            user,
            api_key: None,
            session: None,
            shared_link: Some(shared_link),
        })
    }

    async fn validate_shared_link_slug(
        &self,
        slug: &str,
        path: &str,
        shared_link_tokens: &[String],
    ) -> Result<AuthDto, ErrorResp> {
        let result = shared_links::get_by_slug(&self.db_pool, slug)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Invalid share slug".to_string()))?;
        let (user, shared_link) = result;
        self.require_shared_link_password(&shared_link, path, shared_link_tokens)?;
        Ok(AuthDto {
            user,
            api_key: None,
            session: None,
            shared_link: Some(shared_link),
        })
    }

    fn require_shared_link_password(
        &self,
        shared_link: &crate::models::db::shared_links::AuthSharedLinkDb,
        path: &str,
        shared_link_tokens: &[String],
    ) -> Result<(), ErrorResp> {
        if path == "/api/shared-links/login" {
            return Ok(());
        }

        if let Some(password) = &shared_link.password {
            let link_id = uuid::Uuid::parse_str(&shared_link.id).map_err(|_| {
                ErrorResp::ServerError("Invalid shared link".to_string())
            })?;
            let token = crate::utils::crypto::shared_link_login_token(&link_id, password);
            if !shared_link_tokens.contains(&token) {
                return Err(ErrorResp::Unauthorized("Password required".to_string()));
            }
        }

        Ok(())
    }

    async fn validate_session(&self, token_value: &str) -> Result<AuthDto, ErrorResp> {
        let token = hash_sha256(token_value);
        let session = SessionPO::query_by_token(&self.db_pool, &token)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Invalid user token".to_string()))?;

        let user = AuthUserDb::select_user_by_id(&self.db_pool, &session.user_id)
            .await?
            .ok_or_else(|| ErrorResp::Unauthorized("Invalid user token".to_string()))?;

        let now = Utc::now();
        let has_elevated_permission = session
            .pin_expires_at
            .is_some_and(|expires_at| expires_at > now);

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
            user_id: user_po.id,
            oauth_sid: None,
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
