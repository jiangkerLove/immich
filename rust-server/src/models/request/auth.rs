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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordReq {
    pub password: String,
    pub new_password: String,
    #[serde(default)]
    pub invalidate_sessions: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinCodeSetupReq {
    pub pin_code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinCodeChangeReq {
    pub pin_code: Option<String>,
    pub password: Option<String>,
    pub new_pin_code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinCodeResetReq {
    pub pin_code: Option<String>,
    pub password: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUnlockReq {
    pub pin_code: Option<String>,
    pub password: Option<String>,
}
