use serde::{Deserialize, Serialize};
use crate::db::auth_permission::Permission;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthApiKey {
    pub id: String,
    pub permissions: Vec<Permission>,
}