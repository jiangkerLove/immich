use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserMeReq {
    pub email: Option<String>,
    pub password: Option<String>,
    pub name: Option<String>,
    pub avatar_color: Option<Option<String>>,
}

pub type UserPreferencesUpdateReq = Value;
