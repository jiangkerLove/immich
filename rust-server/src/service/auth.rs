use crate::models::response::response::ErrorResp;
use crate::models::response::user::UserAdminResponse;
use crate::models::response::user_preferences_response::UserPreferenceResponse;
use crate::ext::bcrypt::BcryptCompare;
use crate::models::db::sessions::{AuthSession, NewSession, SessionPO};
use crate::models::db::user_metadata::{UserMetadataKey, UserMetadataPO, UserPreferencePO};
use crate::models::db::users::{AuthUserDb, UserDb};
use crate::models::dto::auth::AuthDto;
use crate::models::request::auth::{LoginCredentialReq, LoginReq};
use crate::models::response::auth::LoginResp;
use crate::utils::crypto::{hash_sha256, random_bytes_as_text};

#[derive(Clone)]
pub struct AuthService {
    db_pool: sqlx::PgPool,
}

impl AuthService {
    pub fn new(db_pool: sqlx::PgPool) -> AuthService {
        AuthService {
            db_pool,
        }
    }

    pub async fn login(&self, login_credential: &LoginCredentialReq, login_details: &LoginReq) -> Result<LoginResp, ErrorResp> {
        let user_option = UserDb::select_full_by_email(&self.db_pool, &login_credential.email).await.map_err(ErrorResp::from)?;
        match user_option {
            None => {
                Err(ErrorResp::Unauthorized(String::from("Incorrect email or password")))
            }
            Some(user_po) => {
                let is_valid = login_credential.password.as_str()
                    .compare_bcrypt(user_po.password.as_str())
                    .is_ok_and(|ok| ok);
                if is_valid {
                    self.create_login_response(user_po, login_details).await
                } else {
                    Err(ErrorResp::Unauthorized(String::from("Incorrect email or password")))
                }
            }
        }
    }

    pub async fn get_me(&self, auth: &AuthDto) -> Result<UserAdminResponse, ErrorResp> {
        let user_opt = UserDb::select_full_by_id(&self.db_pool, &auth.user.id).await.map_err(ErrorResp::from)?;
        match user_opt {
            None => {
                Err(ErrorResp::ServerError("User not found".to_string()))
            }
            Some(user) => {
                Ok(UserAdminResponse {
                    id: String::from(user.id),
                    email: user.email,
                    name: user.name,
                    profile_image_path: user.profile_image_path,
                    avatar_color: user.avatar_color.unwrap_or("".to_string()),
                    profile_changed_at: user.profile_changed_at,
                    storage_label: user.storage_label.unwrap_or("".to_string()),
                    should_change_password: user.should_change_password,
                    is_admin: user.is_admin,
                    created_at: user.created_at,
                    deleted_at: user.deleted_at,
                    updated_at: user.updated_at,
                    oauth_id: user.oauth_id,
                    quota_size_in_bytes: user.quota_size_in_bytes,
                    quota_usage_in_bytes: user.quota_usage_in_bytes,
                    status: user.status.as_str().to_string(),
                    license: None,
                })
            }
        }
    }

    pub async fn get_me_preferences(&self, auth: &AuthDto) -> Result<UserPreferenceResponse, ErrorResp> {
        let mut user_meta = UserMetadataPO::get_meta_data_by_uid(&self.db_pool, &auth.user.id).await?;
        let index_opt = user_meta.iter().position(|x| { x.key == UserMetadataKey::PREFERENCES.as_str() });
        match index_opt {
            None => {
                Ok(UserPreferencePO::default().into())
            }
            Some(index) => {
                let po = user_meta.remove(index);
                Ok(po.value.0.into())
            }
        }
    }

    async fn create_login_response(&self, user_po: UserDb, login_details: &LoginReq) -> Result<LoginResp, ErrorResp> {
        let token = random_bytes_as_text(32);
        let hash_token = hash_sha256(&token);

        let session = NewSession {
            token: hash_token.clone(),
            device_os: login_details.device_os.clone(),
            device_type: login_details.device_type.clone(),
            user_id: user_po.id.clone(),
        };

        let _ = session.insert(&self.db_pool).await.map_err(ErrorResp::from)?;

        Ok(LoginResp {
            access_token: token,
            user_id: user_po.id.clone(),
            user_email: user_po.email.clone(),
            name: user_po.name.clone(),
            is_admin: user_po.is_admin,
            profile_image_path: user_po.profile_image_path.clone(),
            should_change_password: user_po.should_change_password,
        })
    }

    pub(crate) async fn validate_session(&self, token_value: &String) -> Result<AuthDto, ErrorResp> {
        let token = hash_sha256(token_value.as_str());
        let session_opt = SessionPO::query_by_token(&self.db_pool, &token).await.map_err(ErrorResp::from)?;
        match session_opt {
            None => {
                Err(ErrorResp::Unauthorized(String::from("Authentication required")))
            }
            Some(session) => {
                let auth_user = AuthUserDb::select_user_by_id(&self.db_pool, &session.user_id).await.map_err(ErrorResp::from)?;
                match auth_user {
                    None => {
                        Err(ErrorResp::Unauthorized(String::from("Authentication required")))
                    }
                    Some(user) => {
                        Ok(AuthDto {
                            user,
                            api_key: None,
                            session: AuthSession { id: "".to_string(), has_elevated_permission: false }.into(),
                            shared_link: None,
                        })
                    }
                }
            }
        }
    }
}