use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;

pub fn is_granted(requested: &Permission, current: &[Permission]) -> bool {
    if current.iter().any(|p| *p == Permission::All) {
        return true;
    }
    current.iter().any(|p| p == requested)
}

pub fn are_permissions_granted(requested: &[String], current: &[Permission]) -> bool {
    requested.iter().all(|permission| {
        Permission::from_str(permission)
            .map(|permission| is_granted(&permission, current))
            .unwrap_or(false)
    })
}

/// Match TS `AuthService.authenticate`: API keys always need the named permission
/// (even for admin users). Session/cookie auth does not check named permissions here.
pub fn require_permission(auth: &AuthDto, permission: Permission) -> Result<(), ErrorResp> {
    if let Some(api_key) = &auth.api_key {
        if is_granted(&permission, &api_key.permissions) {
            return Ok(());
        }
        return Err(ErrorResp::Forbidden(format!(
            "Missing required permission: {}",
            permission.as_str()
        )));
    }

    // Shared-link / session auth is validated at the resource layer (or adminRoute).
    Ok(())
}

pub fn require_admin(auth: &AuthDto) -> Result<(), ErrorResp> {
    if auth.user.is_admin {
        Ok(())
    } else {
        Err(ErrorResp::Forbidden("Forbidden".to_string()))
    }
}

/// `@Authenticated({ admin: true })` with no named permission defaults to `Permission.All`.
pub fn require_admin_all(auth: &AuthDto) -> Result<(), ErrorResp> {
    require_admin(auth)?;
    require_permission(auth, Permission::All)?;
    Ok(())
}

pub fn require_no_shared_link(auth: &AuthDto) -> Result<(), ErrorResp> {
    if auth.shared_link.is_some() {
        Err(ErrorResp::Forbidden("Forbidden".to_string()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::db::api_key::AuthApiKey;
    use crate::models::db::users::AuthUserDb;

    fn user(is_admin: bool) -> AuthUserDb {
        AuthUserDb {
            id: uuid::Uuid::new_v4(),
            is_admin,
            name: "Admin".into(),
            email: "admin@example.com".into(),
            quota_usage_in_bytes: 0,
            quota_size_in_bytes: None,
        }
    }

    #[test]
    fn admin_api_key_still_needs_named_permission() {
        let auth = AuthDto {
            user: user(true),
            api_key: Some(AuthApiKey {
                id: uuid::Uuid::new_v4().to_string(),
                permissions: vec![Permission::AssetRead],
            }),
            session: None,
            shared_link: None,
        };
        assert!(require_permission(&auth, Permission::LibraryRead).is_err());
        assert!(require_permission(&auth, Permission::AssetRead).is_ok());
    }

    #[test]
    fn admin_session_skips_named_permission() {
        let auth = AuthDto {
            user: user(true),
            api_key: None,
            session: None,
            shared_link: None,
        };
        assert!(require_permission(&auth, Permission::LibraryRead).is_ok());
    }
}
