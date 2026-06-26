use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImmichCookie {
    AccessToken,
    AuthType,
    IsAuthenticated,
    SharedLinkToken,
    OAuthState,
    OAuthCodeVerifier,
    MaintenanceToken,
}

impl ImmichCookie {
    /// 获取枚举对应的字符串键名
    pub fn as_str(&self) -> &'static str {
        match self {
            ImmichCookie::AccessToken => "immich_access_token",
            ImmichCookie::AuthType => "immich_auth_type",
            ImmichCookie::IsAuthenticated => "immich_is_authenticated",
            ImmichCookie::SharedLinkToken => "immich_shared_link_token",
            ImmichCookie::OAuthState => "immich_oauth_state",
            ImmichCookie::OAuthCodeVerifier => "immich_oauth_code_verifier",
            ImmichCookie::MaintenanceToken => "immich_maintenance_token",
        }
    }

    /// 从字符串解析 `ImmichCookie` 枚举
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "immich_access_token" => Some(ImmichCookie::AccessToken),
            "immich_auth_type" => Some(ImmichCookie::AuthType),
            "immich_is_authenticated" => Some(ImmichCookie::IsAuthenticated),
            "immich_shared_link_token" => Some(ImmichCookie::SharedLinkToken),
            "immich_oauth_state" => Some(ImmichCookie::OAuthState),
            "immich_oauth_code_verifier" => Some(ImmichCookie::OAuthCodeVerifier),
            "immich_maintenance_token" => Some(ImmichCookie::MaintenanceToken),
            _ => None,
        }
    }
}

/// 解析 Cookie 字符串，并返回一个 `HashMap<ImmichCookie, String>`
pub fn parse_immich_cookies(cookie_str: &str) -> HashMap<ImmichCookie, String> {
    let mut cookies = HashMap::new();

    for pair in cookie_str.split(';') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }

        let mut kv = pair.splitn(2, '=');

        let key_opt = kv.next();
        let value_opt = kv.next();
        if key_opt.is_none() || value_opt.is_none() {
            continue;
        }
        let key = key_opt.unwrap().trim();
        let value = value_opt.unwrap().trim();

        if let Some(cookie_enum) = ImmichCookie::from_str(key) {
            cookies.insert(cookie_enum, value.to_string());
        }
    }

    cookies
}