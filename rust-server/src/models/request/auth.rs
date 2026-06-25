use serde::Deserialize;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginReq {
    pub is_secure: bool,
    pub client_ip: String,
    pub device_type: String,
    pub device_os: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginCredentialReq {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignUpReq {
    pub email: String,
    pub password: String,
    pub name: String,
}
