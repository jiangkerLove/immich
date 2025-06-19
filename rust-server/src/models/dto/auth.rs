use serde::{Deserialize, Serialize};
use crate::models::db::api_key::AuthApiKey;
use crate::models::db::sessions::AuthSession;
use crate::models::db::shared_links::AuthSharedLinkDb;
use crate::models::db::users::AuthUserDb;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthDto {
    pub user: AuthUserDb,
    pub api_key: Option<AuthApiKey>,
    pub session: Option<AuthSession>,
    pub shared_link: Option<AuthSharedLinkDb>,
}