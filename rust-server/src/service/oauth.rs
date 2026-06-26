use openidconnect::core::{CoreClient, CoreProviderMetadata, CoreResponseType, CoreUserInfoClaims};
use openidconnect::{
    reqwest::async_http_client, AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret,
    CsrfToken, IssuerUrl, Nonce, OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenResponse,
};
use sqlx::PgPool;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

use crate::constants::MOBILE_REDIRECT;
use crate::ext::bcrypt::hash_bcrypt;
use crate::models::db::sessions::{NewSession, SessionPO};
use crate::models::db::system_metadata::{get_oauth_config, OAuthConfig};
use crate::models::db::user_metadata::UserMetadataPO;
use crate::models::db::users::{map_user_admin_with_license, NewUserDb, UserDb};
use crate::models::dto::auth::AuthDto;
use crate::models::request::auth::LoginReq;
use crate::models::response::auth::LoginResp;
use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::utils::crypto::{hash_sha256, random_bytes_as_text};


#[derive(Clone)]
pub struct OAuthService {
    pool: PgPool,
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
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn authorize(&self, dto: &OAuthConfigReq) -> Result<OAuthAuthorizeResp, ErrorResp> {
        let oauth = self.load_oauth().await?;
        let client = self.build_client(&oauth, &dto.redirect_uri).await?;

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let (auth_url, csrf_state, _nonce) = client
            .authorize_url(
                AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(Scope::new(oauth.scope.clone()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        Ok(OAuthAuthorizeResp {
            url: auth_url.to_string(),
            state: dto.state.clone().unwrap_or_else(|| csrf_state.secret().clone()),
            code_verifier: Some(pkce_verifier.secret().clone()),
        })
    }

    pub async fn callback(
        &self,
        dto: &OAuthCallbackReq,
        login_details: &LoginReq,
    ) -> Result<LoginResp, ErrorResp> {
        let oauth = self.load_oauth().await?;
        let callback_url = url::Url::parse(&dto.url)
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

        let pkce_verifier = dto
            .code_verifier
            .as_ref()
            .map(|v| PkceCodeVerifier::new(v.clone()));

        let token_response = client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier.unwrap_or_else(|| PkceCodeVerifier::new(String::new())))
            .request_async(async_http_client)
            .await
            .map_err(|_| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?;

        let userinfo: CoreUserInfoClaims = client
            .user_info(token_response.access_token().to_owned(), None)
            .map_err(|_| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?
            .request_async(async_http_client)
            .await
            .map_err(|_| ErrorResp::BadRequest("OAuth authentication failed".to_string()))?;

        let sub = userinfo
            .subject()
            .as_str()
            .to_string();
        let email = userinfo
            .email()
            .map(|m| m.as_str().trim().to_lowercase())
            .filter(|e| !e.is_empty());

        let user = self.find_or_register_user(&oauth, &sub, email.as_deref()).await?;
        let oauth_sid = token_response
            .id_token()
            .and_then(|token| extract_sid_from_jwt(token.to_string().as_str()));
        self.create_login_response(user, login_details, oauth_sid).await
    }

    pub async fn link(
        &self,
        auth: &AuthDto,
        dto: &OAuthCallbackReq,
    ) -> Result<UserAdminResponse, ErrorResp> {
        let oauth = self.load_oauth().await?;
        let callback_url = url::Url::parse(&dto.url)
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

        let pkce_verifier = dto
            .code_verifier
            .as_ref()
            .map(|v| PkceCodeVerifier::new(v.clone()));

        let token_response = client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier.unwrap_or_else(|| PkceCodeVerifier::new(String::new())))
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
                sqlx::query(r#"UPDATE session SET "oauthSid" = $1 WHERE id = $2"#)
                    .bind(oauth_sid.as_deref())
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
                sqlx::query(r#"UPDATE session SET "oauthSid" = NULL WHERE id = $1"#)
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
        format!("{MOBILE_REDIRECT}?{}", request_url.split('?').nth(1).unwrap_or(""))
    }

    pub async fn backchannel_logout(&self, logout_token: &str) -> Result<(), ErrorResp> {
        let oauth = self.load_oauth().await?;
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

        SessionPO::invalidate_oauth(
            &self.pool,
            claims.sid.as_deref(),
            claims.sub.as_deref(),
        )
        .await?;

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

        Ok(CoreClient::from_provider_metadata(metadata, client_id, client_secret).set_redirect_uri(redirect))
    }

    async fn find_or_register_user(
        &self,
        oauth: &OAuthConfig,
        oauth_id: &str,
        email: Option<&str>,
    ) -> Result<UserDb, ErrorResp> {
        if let Some(user) = UserDb::select_by_oauth_id(&self.pool, oauth_id).await? {
            return Ok(user);
        }

        if let Some(email) = email {
            if let Some(user) = UserDb::select_full_by_email(&self.pool, email).await? {
                if user.oauth_id.is_empty() {
                    sqlx::query(r#"UPDATE "user" SET "oauthId" = $1 WHERE id = $2"#)
                        .bind(oauth_id)
                        .bind(user.id)
                        .execute(&self.pool)
                        .await?;
                    return UserDb::select_full_by_id(&self.pool, &user.id)
                        .await?
                        .ok_or_else(|| ErrorResp::ServerError("User not found".to_string()));
                }
                return Err(ErrorResp::BadRequest("OAuth authentication failed".to_string()));
            }
        }

        if !oauth.auto_register {
            return Err(ErrorResp::BadRequest("OAuth authentication failed".to_string()));
        }

        let email = email.ok_or_else(|| {
            ErrorResp::BadRequest("OAuth profile does not have an email address".to_string())
        })?;

        let password = hash_bcrypt(&random_bytes_as_text(32))
            .map_err(|e| ErrorResp::ServerError(e.to_string()))?;

        let user = UserDb::insert(
            &self.pool,
            &NewUserDb {
                email: email.to_string(),
                password,
                name: email.to_string(),
                is_admin: false,
                storage_label: None,
            },
        )
        .await
        .map_err(ErrorResp::from)?;

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
    ) -> Result<LoginResp, ErrorResp> {
        let token = random_bytes_as_text(32);
        let hash_token = hash_sha256(&token);
        let is_onboarded = UserMetadataPO::is_onboarded(&self.pool, &user_po.id).await?;

        let session = NewSession {
            token: hash_token,
            device_os: login_details.device_os.clone(),
            device_type: login_details.device_type.clone(),
            user_id: user_po.id,
            oauth_sid,
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
}

#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<serde_json::Value>,
}

fn extract_sid_from_jwt(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value
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
