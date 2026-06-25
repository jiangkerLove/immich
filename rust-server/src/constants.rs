/// Bcrypt hash used during login to prevent timing-based user enumeration.
pub const LOGIN_DUMMY_HASH: &str =
    "$2b$10$abcdefghijklmnopqrstuuABCDEFGHIJKLMNOPQRSTUVWXYZabcde";

pub const LOGIN_URL: &str = "/auth/login?autoLaunch=0";

pub const MOBILE_REDIRECT: &str = "app.immich:///oauth-callback";

pub const SALT_ROUNDS: u32 = 10;

/// Matches server/package.json version for API compatibility.
pub const SERVER_VERSION: &str = "3.0.0-rc.2";
