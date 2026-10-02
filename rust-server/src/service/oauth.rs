use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use openidconnect::core::{CoreClient, CoreProviderMetadata, CoreResponseType, CoreUserInfoClaims};
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce,
    OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
    reqwest::async_http_client,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::constants::MOBILE_REDIRECT;
use crate::ext::bcrypt::hash_bcrypt;
use crate::models::db::sessions::{NewSession, SessionPO};
use crate::models::db::system_metadata::{OAuthConfig, get_oauth_config};
use crate::models::db::user_metadata::UserMetadataPO;
use crate::models::db::users::{NewUserDb, UserDb, map_user_admin_with_license};
use crate::models::dto::auth::AuthDto;
use crate::models::request::auth::LoginReq;
use crate::models::response::auth::LoginResp;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::service::job::JobService;
use crate::service::websocket::WebSocketHub;
use crate::utils::crypto::{hash_sha256, random_bytes_as_text};
use crate::utils::storage::StoragePaths;

#[derive(Clone)]
pub struct OAuthService {
    pool: PgPool,
    websocket: WebSocketHub,
    storage: StoragePaths,
    jobs: JobService,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthConfigReq {
    pub redirect_uri: String,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
}

#[derive(serde::Serialize)]
pub struct OAuthAuthorizeResp {
    pub url: String,
    #[serde(skip)]
    pub state: String,
    #[serde(skip)]
    pub code_verifier: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthCallbackReq {
    pub url: String,
    pub state: Option<String>,
    pub code_verifier: Option<String>,
}

impl OAuthService {
    pub fn new(
        pool: PgPool,
        websocket: WebSocketHub,
        storage: StoragePaths,
        jobs: JobService,
    ) -> Self {
        Self {
            pool,
            websocket,
            storage,
            jobs,
        }
    }

    pub async fn authorize(&self, dto: &OAuthConfigReq) -> Result<OAuthAuthorizeResp, ErrorResp> {
        let oauth = self.load_oauth().await?;
        let redirect_uri = resolve_oauth_redirect(&oauth, &dto.redirect_uri);
        let client = self.build_client(&oauth, &redirect_uri).await?;

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client
            .authorize_url(
                AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(Scope::new(oauth.scope.clone()))
            .set_pkce_challenge(pkce_challenge);
        if !oauth.prompt.is_empty() {
            request = request.add_extra_param("prompt", oauth.prompt.clone());
        }
        let (mut auth_url, csrf_state, _nonce) = request.url();
        let client_challenge = dto
            .code_challenge
            .as_deref()
            .filter(|value| !value.is_empty());
        if let Some(challenge) = client_challenge {
            let pairs: Vec<(String, String)> = auth_url
                .query_pairs()
                .map(|(key, value)| {
                    if key == "code_challenge" {
                        (key.to_string(), challenge.to_string())
                    } else {
                        (key.to_string(), value.to_string())
                    }
                })
                .collect();
            auth_url.query_pairs_mut().clear().extend_pairs(pairs);
        }

        Ok(OAuthAuthorizeResp {
            url: auth_url.to_string(),
            state: dto
                .state
                .clone()
                .unwrap_or_else(|| csrf_state.secret().clone()),
            code_verifier: if client_challenge.is_some() {
                None
            } else {
                Some(pkce_verifier.secret().clone())
            },
        })
    }

    pub async fn callback(
        &self,
        dto: &OAuthCallbackReq,
        login_details: &LoginReq,
    ) -> Result<LoginResp, ErrorResp> {
        let oauth = self.load_oauth().await?;
        let resolved_url = resolve_oauth_redirect(&oauth, &dto.url);
        let callback_url = url::Url::parse(&resolved_url)
            .map_err(|_| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?;
        let redirect_uri = format!(
            "{}://{}{}",
            callback_url.scheme(),
            callback_url.host_str().unwrap_or(""),
            callback_url.path()
        );

        let client = self.build_client(&oauth, &redirect_uri).await?;
        let code = AuthorizationCode::new(
            callback_url
                .query_pairs()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.to_string())
                .ok_or_else(|| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?,
        );

        let pkce_verifier = require_oauth_pkce(dto, &callback_url)?;

        let token_response = client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier)
            .request_async(async_http_client)
            .await
            .map_err(|_| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?;

        let userinfo: CoreUserInfoClaims = client
            .user_info(token_response.access_token().to_owned(), None)
            .map_err(|_| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?
            .request_async(async_http_client)
            .await
            .map_err(|_| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?;

        let id_token = token_response.id_token().map(|token| token.to_string());
        let claims = id_token
            .as_deref()
            .and_then(decode_jwt_payload)
            .unwrap_or_default();
        let profile = oauth_profile(&claims, &userinfo);
        let sub = claim_string(&profile, "sub")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?;
        let email = claim_string(&profile, "email")
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty());
        let mut user = self
            .find_or_register_user(&oauth, &sub, email.as_deref(), &profile)
            .await?;
        let picture = claim_string(&profile, "picture").filter(|url| !url.is_empty());
        if user.profile_image_path.is_empty() {
            if let Some(url) = picture {
                if let Some(updated) = self.sync_profile_picture(&user, &url).await {
                    user = updated;
                }
            }
        }
        let oauth_sid = id_token.as_deref().and_then(extract_sid_from_jwt);
        self.create_login_response(user, login_details, oauth_sid, id_token)
            .await
    }

    pub async fn link(
        &self,
        auth: &AuthDto,
        dto: &OAuthCallbackReq,
    ) -> Result<UserAdminResponse, ErrorResp> {
        let oauth = self.load_oauth().await?;
        let resolved_url = resolve_oauth_redirect(&oauth, &dto.url);
        let callback_url = url::Url::parse(&resolved_url)
            .map_err(|_| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?;
        let redirect_uri = format!(
            "{}://{}{}",
            callback_url.scheme(),
            callback_url.host_str().unwrap_or(""),
            callback_url.path()
        );
        let client = self.build_client(&oauth, &redirect_uri).await?;

        let code = AuthorizationCode::new(
            callback_url
                .query_pairs()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.to_string())
                .ok_or_else(|| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?,
        );

        let pkce_verifier = require_oauth_pkce(dto, &callback_url)?;

        let token_response = client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier)
            .request_async(async_http_client)
            .await
            .map_err(|_| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?;

        let userinfo: CoreUserInfoClaims = client
            .user_info(token_response.access_token().to_owned(), None)
            .map_err(|_| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?
            .request_async(async_http_client)
            .await
            .map_err(|_| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?;

        let oauth_id = userinfo.subject().as_str();
        let oauth_sid = token_response
            .id_token()
            .and_then(|token| extract_sid_from_jwt(token.to_string().as_str()));
        if let Some(duplicate) = UserDb::select_by_oauth_id(&self.pool, oauth_id).await? {
            if duplicate.id != auth.user.id {
                return Err(ErrorResp::BadRequest(
                    "This OAuth account has already been linked to another user.".to_string(),
                ));
            }
        }

        sqlx::query(r#"UPDATE "user" SET "oauthId" = $1 WHERE id = $2"#)
            .bind(oauth_id)
            .bind(auth.user.id)
            .execute(&self.pool)
            .await?;

        if let Some(session) = &auth.session {
            if let Ok(session_id) = uuid::Uuid::parse_str(&session.id) {
                let id_token = token_response.id_token().map(|token| token.to_string());
                sqlx::query(
                    r#"UPDATE session SET "oauthSid" = $1, "oauthBearerToken" = $2 WHERE id = $3"#,
                )
                .bind(oauth_sid.as_deref())
                .bind(id_token.as_deref())
                .bind(session_id)
                .execute(&self.pool)
                .await?;
            }
        }

        let user = UserDb::select_full_by_id(&self.pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::ServerError("User not found".to_string()))?;
        Ok(map_user_admin_with_license(&self.pool, user).await?)
    }

    pub async fn unlink(&self, auth: &AuthDto) -> Result<UserAdminResponse, ErrorResp> {
        if let Some(session) = &auth.session {
            if let Ok(session_id) = uuid::Uuid::parse_str(&session.id) {
                sqlx::query(
                    r#"UPDATE session SET "oauthSid" = NULL, "oauthBearerToken" = NULL WHERE id = $1"#,
                )
                    .bind(session_id)
                    .execute(&self.pool)
                    .await?;
            }
        }

        sqlx::query(r#"UPDATE "user" SET "oauthId" = '' WHERE id = $1"#)
            .bind(auth.user.id)
            .execute(&self.pool)
            .await?;

        let user = UserDb::select_full_by_id(&self.pool, &auth.user.id)
            .await?
            .ok_or_else(|| ErrorResp::ServerError("User not found".to_string()))?;
        Ok(map_user_admin_with_license(&self.pool, user).await?)
    }

    pub fn mobile_redirect(request_url: &str) -> String {
        format!(
            "{MOBILE_REDIRECT}?{}",
            request_url.split('?').nth(1).unwrap_or("")
        )
    }

    pub async fn backchannel_logout(&self, logout_token: &str) -> Result<(), ErrorResp> {
        let oauth = self.load_oauth().await?;
        if !oauth.enabled {
            return Err(ErrorResp::BadRequest(
                "Received backchannel logout request but OAuth is not enabled".to_string(),
            ));
        }

        let claims = self
            .validate_logout_token(&oauth, logout_token)
            .await
            .map_err(|_| {
                ErrorResp::BadRequest(
                    "Error backchannel logout: token validation failed".to_string(),
                )
            })?;

        if claims.sub.is_none() && claims.sid.is_none() {
            return Err(ErrorResp::BadRequest(
                "Invalid logout token: it must contain either a sub or a sid claim".to_string(),
            ));
        }

        let deleted_session_ids =
            SessionPO::invalidate_oauth(&self.pool, claims.sid.as_deref(), claims.sub.as_deref())
                .await?;

        for session_id in deleted_session_ids {
            self.websocket.emit_session_delete(session_id);
        }

        Ok(())
    }

    async fn validate_logout_token(
        &self,
        oauth: &OAuthConfig,
        logout_token: &str,
    ) -> Result<LogoutClaims, String> {
        let algorithm = map_signing_algorithm(&oauth.signing_algorithm)?;
        let decoding_key = if oauth.signing_algorithm.starts_with("HS") {
            DecodingKey::from_secret(oauth.client_secret.as_bytes())
        } else {
            let header = decode_header(logout_token).map_err(|err| err.to_string())?;
            let kid = header
                .kid
                .ok_or_else(|| "Missing kid in logout token".to_string())?;
            let jwks = self.fetch_jwks(&oauth.issuer_url).await?;
            let jwk = jwks
                .keys
                .into_iter()
                .find(|key| key.get("kid").and_then(|value| value.as_str()) == Some(kid.as_str()))
                .ok_or_else(|| "Unable to find matching JWK".to_string())?;
            decoding_key_from_jwk(&jwk)?
        };

        let mut validation = Validation::new(algorithm);
        validation.set_audience(&[oauth.client_id.as_str()]);
        validation.set_issuer(&[oauth.issuer_url.as_str()]);
        validation.validate_exp = true;
        validation.leeway = 5;

        let token_data = decode::<LogoutClaims>(logout_token, &decoding_key, &validation)
            .map_err(|err| err.to_string())?;
        let claims = token_data.claims;

        if let Some(issued_at) = claims.iat {
            let now = chrono::Utc::now().timestamp();
            if now.saturating_sub(issued_at) > 120 {
                return Err("Logout token is too old".to_string());
            }
        }

        if claims
            .events
            .as_ref()
            .and_then(|events| events.get("http://schemas.openid.net/event/backchannel-logout"))
            .is_none()
        {
            return Err("Missing backchannel-logout event claim".to_string());
        }

        if claims.nonce.is_some() {
            return Err("Logout token must not contain a nonce".to_string());
        }

        Ok(claims)
    }

    async fn fetch_jwks(&self, issuer_url: &str) -> Result<JwksResponse, String> {
        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            issuer_url.trim_end_matches('/')
        );
        let discovery: serde_json::Value = reqwest::get(&discovery_url)
            .await
            .map_err(|err| err.to_string())?
            .json()
            .await
            .map_err(|err| err.to_string())?;
        let jwks_uri = discovery
            .get("jwks_uri")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "Unable to get JWKS URI".to_string())?;
        reqwest::get(jwks_uri)
            .await
            .map_err(|err| err.to_string())?
            .json::<JwksResponse>()
            .await
            .map_err(|err| err.to_string())
    }

    async fn sync_profile_picture(&self, user: &UserDb, url: &str) -> Option<UserDb> {
        let bytes = match reqwest::get(url).await {
            Ok(response) if response.status().is_success() => match response.bytes().await {
                Ok(bytes) => bytes,
                Err(err) => {
                    tracing::warn!("Unable to sync oauth profile picture: {err}");
                    return None;
                }
            },
            Ok(response) => {
                tracing::warn!(
                    "Unable to sync oauth profile picture: {}",
                    response.status()
                );
                return None;
            }
            Err(err) => {
                tracing::warn!("Unable to sync oauth profile picture: {err}");
                return None;
            }
        };

        let path = match crate::utils::profile_image::generate_profile_image(
            &self.pool,
            &self.storage,
            &user.id,
            &bytes,
        )
        .await
        {
            Ok(path) => path,
            Err(err) => {
                tracing::warn!("Unable to sync oauth profile picture: {err}");
                return None;
            }
        };

        match UserDb::update_profile_image(&self.pool, &user.id, &path.to_string_lossy()).await {
            Ok(updated) => {
                if !user.profile_image_path.is_empty() {
                    let _ = self
                        .jobs
                        .queue_file_delete(&[user.profile_image_path.as_str()])
                        .await;
                }
                Some(updated)
            }
            Err(err) => {
                tracing::warn!("Unable to sync oauth profile picture: {err}");
                None
            }
        }
    }

    async fn load_oauth(&self) -> Result<OAuthConfig, ErrorResp> {
        let oauth = get_oauth_config(&self.pool)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("OAuth is not enabled".to_string()))?;
        if !oauth.enabled {
            return Err(ErrorResp::BadRequest("OAuth is not enabled".to_string()));
        }
        Ok(oauth)
    }

    async fn build_client(
        &self,
        oauth: &OAuthConfig,
        redirect_uri: &str,
    ) -> Result<CoreClient, ErrorResp> {
        let issuer = IssuerUrl::new(oauth.issuer_url.clone())
            .map_err(|_| ErrorResp::BadRequest("Invalid OAuth issuer".to_string()))?;
        let metadata = CoreProviderMetadata::discover_async(issuer, async_http_client)
            .await
            .map_err(|_| ErrorResp::BadRequest("OAuth discovery failed".to_string()))?;

        let client_id = ClientId::new(oauth.client_id.clone());
        let client_secret = if oauth.client_secret.is_empty() {
            None
        } else {
            Some(ClientSecret::new(oauth.client_secret.clone()))
        };

        let redirect = RedirectUrl::new(redirect_uri.to_string())
            .map_err(|_| ErrorResp::BadRequest("Invalid redirect URI".to_string()))?;

        Ok(
            CoreClient::from_provider_metadata(metadata, client_id, client_secret)
                .set_redirect_uri(redirect),
        )
    }

    async fn find_or_register_user(
        &self,
        oauth: &OAuthConfig,
        oauth_id: &str,
        email: Option<&str>,
        claims: &serde_json::Value,
    ) -> Result<UserDb, ErrorResp> {
        if let Some(mut user) = UserDb::select_by_oauth_id(&self.pool, oauth_id).await? {
            if let Some(is_admin) = role_is_admin(claims, &oauth.role_claim) {
                if is_admin != user.is_admin {
                    sqlx::query(r#"UPDATE "user" SET "isAdmin" = $1 WHERE id = $2"#)
                        .bind(is_admin)
                        .bind(user.id)
                        .execute(&self.pool)
                        .await?;
                    user.is_admin = is_admin;
                }
            }
            return Ok(user);
        }

        let email = email.map(crate::service::auth::normalize_email);
        if let Some(email) = email.as_deref() {
            if let Some(user) = UserDb::select_full_by_email(&self.pool, email).await? {
                if user.oauth_id.is_empty() {
                    sqlx::query(r#"UPDATE "user" SET "oauthId" = $1 WHERE id = $2"#)
                        .bind(oauth_id)
                        .bind(user.id)
                        .execute(&self.pool)
                        .await?;
                    let mut linked = UserDb::select_full_by_id(&self.pool, &user.id)
                        .await?
                        .ok_or_else(|| ErrorResp::ServerError("User not found".to_string()))?;
                    if let Some(is_admin) = role_is_admin(claims, &oauth.role_claim) {
                        if is_admin != linked.is_admin {
                            sqlx::query(r#"UPDATE "user" SET "isAdmin" = $1 WHERE id = $2"#)
                                .bind(is_admin)
                                .bind(linked.id)
                                .execute(&self.pool)
                                .await?;
                            linked.is_admin = is_admin;
                        }
                    }
                    return Ok(linked);
                }
                return Err(ErrorResp::BadRequest(
                    "OAuth authentication failed".to_string(),
                ));
            }
        }

        if !oauth.auto_register {
            return Err(ErrorResp::BadRequest(
                "OAuth authentication failed".to_string(),
            ));
        }

        let email = email.ok_or_else(|| {
            ErrorResp::BadRequest("OAuth profile does not have an email address".to_string())
        })?;

        let password = hash_bcrypt(&random_bytes_as_text(32))
            .map_err(|e| ErrorResp::ServerError(e.to_string()))?;

        let storage_label = claim_string(claims, &oauth.storage_label_claim)
            .filter(|value| !value.is_empty())
            .map(|value| crate::utils::storage::sanitize_storage_label(&value))
            .filter(|value| !value.is_empty());
        let quota = claim_quota_bytes(
            claims,
            &oauth.storage_quota_claim,
            oauth.default_storage_quota,
        );
        let user = UserDb::insert(
            &self.pool,
            &NewUserDb {
                email: email.to_string(),
                password,
                name: claim_name(claims, &email),
                is_admin: role_is_admin(claims, &oauth.role_claim).unwrap_or(false),
                storage_label,
            },
        )
        .await
        .map_err(ErrorResp::from)?;
        if let Some(quota) = quota {
            sqlx::query(r#"UPDATE "user" SET "quotaSizeInBytes" = $1 WHERE id = $2"#)
                .bind(quota)
                .bind(user.id)
                .execute(&self.pool)
                .await?;
        }

        crate::utils::telemetry::add_users_total(1);

        sqlx::query(r#"UPDATE "user" SET "oauthId" = $1 WHERE id = $2"#)
            .bind(oauth_id)
            .bind(user.id)
            .execute(&self.pool)
            .await?;

        UserDb::select_full_by_id(&self.pool, &user.id)
            .await?
            .ok_or_else(|| ErrorResp::ServerError("User not found".to_string()))
    }

    async fn create_login_response(
        &self,
        user_po: UserDb,
        login_details: &LoginReq,
        oauth_sid: Option<String>,
        oauth_bearer_token: Option<String>,
    ) -> Result<LoginResp, ErrorResp> {
        let token = random_bytes_as_text(32);
        let hash_token = hash_sha256(&token);
        let is_onboarded = UserMetadataPO::is_onboarded(&self.pool, &user_po.id).await?;

        let session = NewSession {
            token: hash_token,
            device_os: login_details.device_os.clone(),
            device_type: login_details.device_type.clone(),
            app_version: login_details.app_version.clone(),
            user_id: user_po.id,
            oauth_sid,
            oauth_bearer_token,
        };
        session.insert(&self.pool).await?;

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

#[derive(Debug, Deserialize)]
struct LogoutClaims {
    sub: Option<String>,
    sid: Option<String>,
    nonce: Option<String>,
    events: Option<serde_json::Value>,
    iat: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<serde_json::Value>,
}

pub(crate) async fn discover_end_session_endpoint(issuer_url: &str) -> Option<String> {
    if issuer_url.is_empty() {
        return None;
    }
    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        issuer_url.trim_end_matches('/')
    );
    let discovery: serde_json::Value =
        reqwest::get(&discovery_url).await.ok()?.json().await.ok()?;
    discovery
        .get("end_session_endpoint")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn resolve_oauth_redirect(oauth: &OAuthConfig, url: &str) -> String {
    if !oauth.mobile_override_enabled || oauth.mobile_redirect_uri.is_empty() {
        return url.to_string();
    }
    let Some(start) = url.find("app.immich:") else {
        return url.to_string();
    };
    let after = &url[start + "app.immich:".len()..];
    let after = after.trim_start_matches('/');
    let Some(rest) = after.strip_prefix("oauth-callback") else {
        return url.to_string();
    };
    format!("{}{rest}", oauth.mobile_redirect_uri)
}

fn oauth_profile(claims: &serde_json::Value, userinfo: &CoreUserInfoClaims) -> serde_json::Value {
    if claims.get("email").is_some() {
        return claims.clone();
    }
    serde_json::to_value(userinfo).unwrap_or_else(|_| serde_json::json!({}))
}

fn decode_jwt_payload(id_token: &str) -> Option<serde_json::Value> {
    let payload = id_token.split('.').nth(1)?;
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn claim_string(claims: &serde_json::Value, key: &str) -> Option<String> {
    if key.is_empty() {
        return None;
    }
    claims.get(key).and_then(|value| match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

fn claim_name(claims: &serde_json::Value, email: &str) -> String {
    claim_string(claims, "name")
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let given = claim_string(claims, "given_name").unwrap_or_default();
            let family = claim_string(claims, "family_name").unwrap_or_default();
            let combined = format!("{given} {family}").trim().to_string();
            if combined.is_empty() {
                None
            } else {
                Some(combined)
            }
        })
        .or_else(|| claim_string(claims, "preferred_username"))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| email.to_string())
}

fn role_is_admin(claims: &serde_json::Value, role_claim: &str) -> Option<bool> {
    if role_claim.is_empty() {
        return None;
    }
    let value = claims.get(role_claim)?;
    let roles: Vec<String> = match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        serde_json::Value::String(role) => vec![role.clone()],
        _ => return None,
    };
    if roles.iter().any(|role| role == "admin") {
        Some(true)
    } else if roles.iter().any(|role| role == "user") {
        Some(false)
    } else {
        None
    }
}

fn claim_quota_bytes(
    claims: &serde_json::Value,
    quota_claim: &str,
    default_gib: Option<i64>,
) -> Option<i64> {
    const GIB: i64 = 1024 * 1024 * 1024;
    let claimed = if quota_claim.is_empty() {
        None
    } else {
        claims.get(quota_claim).and_then(|value| match value {
            serde_json::Value::Number(number) => number.as_i64().filter(|n| *n >= 0),
            serde_json::Value::String(text) => text.parse::<i64>().ok().filter(|n| *n >= 0),
            _ => None,
        })
    };
    claimed.or(default_gib).map(|gib| gib.saturating_mul(GIB))
}

fn extract_sid_from_jwt(id_token: &str) -> Option<String> {
    decode_jwt_payload(id_token)?
        .get("sid")
        .and_then(|sid| sid.as_str())
        .map(str::to_string)
}

fn map_signing_algorithm(value: &str) -> Result<Algorithm, String> {
    match value {
        "HS256" => Ok(Algorithm::HS256),
        "HS384" => Ok(Algorithm::HS384),
        "HS512" => Ok(Algorithm::HS512),
        "RS256" => Ok(Algorithm::RS256),
        "RS384" => Ok(Algorithm::RS384),
        "RS512" => Ok(Algorithm::RS512),
        "ES256" => Ok(Algorithm::ES256),
        "ES384" => Ok(Algorithm::ES384),
        other => Err(format!("Unsupported signing algorithm: {other}")),
    }
}

fn decoding_key_from_jwk(jwk: &serde_json::Value) -> Result<DecodingKey, String> {
    let n = jwk
        .get("n")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Missing RSA modulus".to_string())?;
    let e = jwk
        .get("e")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Missing RSA exponent".to_string())?;
    DecodingKey::from_rsa_components(n, e).map_err(|err| err.to_string())
}

fn require_oauth_pkce(
    dto: &OAuthCallbackReq,
    callback_url: &url::Url,
) -> Result<PkceCodeVerifier, ErrorResp> {
    let expected = dto.state.as_deref().filter(|state| !state.is_empty());
    let Some(expected) = expected else {
        return Err(ErrorResp::BadRequest("OAuth state is missing".to_string()));
    };
    if let Some(url_state) = callback_url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.to_string())
    {
        if url_state != expected {
            return Err(ErrorResp::BadRequest(
                "OAuth authentication failed".to_string(),
            ));
        }
    }

    let verifier = dto
        .code_verifier
        .as_deref()
        .filter(|verifier| !verifier.is_empty())
        .ok_or_else(|| ErrorResp::BadRequest("OAuth code verifier is missing".to_string()))?;
    Ok(PkceCodeVerifier::new(verifier.to_string()))
}
