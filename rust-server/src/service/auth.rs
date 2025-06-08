use sqlx::{Pool, Postgres};
use crate::db::sessions::NewSession;
use crate::db::users::UserPO;
use crate::dtos::auth::{LoginCredentialDto, LoginDetails, LoginResponseDto};
use crate::dtos::response::{ErrorDto};
use crate::ext::bcrypt::BcryptCompare;
use crate::utils::crypto::{hash_sha256, random_bytes_as_text};

#[derive(Clone)]
pub struct AuthService {}

impl AuthService {
    pub fn new() -> AuthService {
        AuthService {}
    }

    pub async fn login(&self, pool: &Pool<Postgres>, login_credential: &LoginCredentialDto, login_details: &LoginDetails) -> Result<LoginResponseDto, ErrorDto> {
        let user_option = UserPO::select_full_by_email(pool, &login_credential.email).await.map_err(ErrorDto::from)?;
        match user_option {
            None => {
                Err(ErrorDto::Unauthorized(String::from("Incorrect email or password")))
            }
            Some(user_po) => {
                let is_valid = login_credential.password.as_str()
                    .compare_bcrypt(user_po.password.as_str())
                    .is_ok_and(|ok| ok);
                if is_valid {
                    self.create_login_response(pool, user_po, login_details).await
                } else {
                    Err(ErrorDto::Unauthorized(String::from("Incorrect email or password")))
                }
            }
        }
    }

    async fn create_login_response(&self, pool: &Pool<Postgres>, user_po: UserPO, login_details: &LoginDetails) -> Result<LoginResponseDto, ErrorDto> {
        let token = random_bytes_as_text(32);
        let hash_token = hash_sha256(&token);

        let session = NewSession {
            token: hash_token.clone(),
            device_os: login_details.device_os.clone(),
            device_type: login_details.device_type.clone(),
            user_id: user_po.id.clone(),
        };

        let _ = NewSession::insert(pool, &session).await.map_err(ErrorDto::from)?;

        Ok(LoginResponseDto {
            access_token: token,
            user_id: user_po.id.clone(),
            user_email: user_po.email.clone(),
            name: user_po.name.clone(),
            is_admin: user_po.is_admin,
            profile_image_path: user_po.profile_image_path.clone(),
            should_change_password: user_po.should_change_password,
        })
    }
}