use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;

pub fn is_granted(requested: &Permission, current: &[Permission]) -> bool {
    if current.iter().any(|p| *p == Permission::All) {
        return true;
    }
    current.iter().any(|p| p == requested)
}

pub fn require_permission(auth: &AuthDto, permission: Permission) -> Result<(), ErrorResp> {
    if auth.user.is_admin {
        return Ok(());
    }

    if let Some(api_key) = &auth.api_key {
        if is_granted(&permission, &api_key.permissions) {
            return Ok(());
        }
        return Err(ErrorResp::Forbidden(format!(
            "Missing required permission: {}",
            permission.as_str()
        )));
    }

    // Shared-link auth is validated at the resource layer (album/asset access).
    Ok(())
}

pub fn require_admin(auth: &AuthDto) -> Result<(), ErrorResp> {
    if auth.user.is_admin {
        Ok(())
    } else {
        Err(ErrorResp::Forbidden("Forbidden".to_string()))
    }
}

pub fn require_no_shared_link(auth: &AuthDto) -> Result<(), ErrorResp> {
    if auth.shared_link.is_some() {
        Err(ErrorResp::Forbidden("Forbidden".to_string()))
    } else {
        Ok(())
    }
}
