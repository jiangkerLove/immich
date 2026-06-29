use std::collections::HashSet;
use uuid::Uuid;

use crate::models::db::album::{self, AlbumAccessLevel};
use crate::models::db::assets;
use crate::models::db::auth_permission::Permission;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::utils::permission::require_permission;

pub async fn require_asset_access(
    pool: &sqlx::PgPool,
    auth: &AuthDto,
    asset_id: &Uuid,
    permission: Permission,
) -> Result<(), ErrorResp> {
    require_permission(auth, permission.clone())?;

    if let Some(shared_link) = &auth.shared_link {
        if permission == Permission::AssetDownload && !shared_link.allow_download {
            return Err(ErrorResp::BadRequest(
                "Not found or no asset.download access".to_string(),
            ));
        }
        if permission == Permission::AssetUpload && !shared_link.allow_upload {
            return Err(ErrorResp::Unauthorized("Unauthorized".to_string()));
        }

        let link_id = Uuid::parse_str(&shared_link.id)
            .map_err(|_| ErrorResp::Unauthorized("Invalid share key".to_string()))?;

        let allowed = assets::shared_link_has_asset(pool, &link_id, &[*asset_id]).await?;
        if !allowed {
            return Err(ErrorResp::BadRequest(
                "Not found or no asset.read access".to_string(),
            ));
        }
        return Ok(());
    }

    let allowed = assets::owner_has_asset(pool, &auth.user.id, asset_id).await?;
    if !allowed {
        return Err(ErrorResp::BadRequest(format!(
            "Not found or no {} access",
            permission.as_str()
        )));
    }
    Ok(())
}

pub async fn require_assets_access(
    pool: &sqlx::PgPool,
    auth: &AuthDto,
    asset_ids: &[Uuid],
    permission: Permission,
) -> Result<(), ErrorResp> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    require_permission(auth, permission.clone())?;

    if let Some(shared_link) = &auth.shared_link {
        if permission == Permission::AssetDownload && !shared_link.allow_download {
            return Err(ErrorResp::BadRequest(
                "Not found or no asset.download access".to_string(),
            ));
        }

        let link_id = Uuid::parse_str(&shared_link.id)
            .map_err(|_| ErrorResp::Unauthorized("Invalid share key".to_string()))?;
        let allowed = assets::shared_link_accessible_ids(pool, &link_id, asset_ids).await?;
        if allowed.len() != asset_ids.len() {
            return Err(ErrorResp::BadRequest(format!(
                "Not found or no {} access",
                permission.as_str()
            )));
        }
        return Ok(());
    }

    let elevated = auth
        .session
        .as_ref()
        .is_some_and(|session| session.has_elevated_permission);
    let owner_only = matches!(permission, Permission::AssetUpdate | Permission::AssetDelete);
    let allowed = assets::filter_accessible_ids(pool, &auth.user.id, asset_ids, elevated, owner_only).await?;

    if allowed.len() != asset_ids.len() {
        return Err(ErrorResp::BadRequest(format!(
            "Not found or no {} access",
            permission.as_str()
        )));
    }

    Ok(())
}

pub async fn require_album_access(
    pool: &sqlx::PgPool,
    auth: &AuthDto,
    album_id: &Uuid,
    permission: Permission,
) -> Result<(), ErrorResp> {
    require_permission(auth, permission.clone())?;

    if let Some(shared_link) = &auth.shared_link {
        if permission == Permission::AlbumDownload && !shared_link.allow_download {
            return Err(ErrorResp::BadRequest(format!(
                "Not found or no {} access",
                permission.as_str()
            )));
        }

        let link_id = Uuid::parse_str(&shared_link.id)
            .map_err(|_| ErrorResp::Unauthorized("Invalid share key".to_string()))?;

        if album::shared_link_has_album(pool, &link_id, album_id).await? {
            return Ok(());
        }

        return Err(ErrorResp::BadRequest(format!(
            "Not found or no {} access",
            permission.as_str()
        )));
    }

    let level = match permission {
        Permission::AlbumDelete => AlbumAccessLevel::Owner,
        Permission::AlbumUpdate
        | Permission::AlbumAddAsset
        | Permission::AlbumRemoveAsset
        | Permission::AlbumShare => AlbumAccessLevel::Editor,
        Permission::AlbumRead | Permission::AlbumDownload | Permission::AlbumStatistics => {
            AlbumAccessLevel::Member
        }
        _ => {
            return Err(ErrorResp::BadRequest(format!(
                "Unsupported album permission: {}",
                permission.as_str()
            )));
        }
    };

    let allowed = album::has_album_access(pool, &auth.user.id, album_id, level).await?;
    if !allowed {
        return Err(ErrorResp::BadRequest(format!(
            "Not found or no {} access",
            permission.as_str()
        )));
    }

    Ok(())
}

pub async fn check_album_ids_access(
    pool: &sqlx::PgPool,
    auth: &AuthDto,
    album_ids: &[Uuid],
    permission: Permission,
) -> Result<HashSet<Uuid>, ErrorResp> {
    require_permission(auth, permission.clone())?;

    if let Some(shared_link) = &auth.shared_link {
        if permission == Permission::AlbumDownload && !shared_link.allow_download {
            return Ok(HashSet::new());
        }

        let link_id = Uuid::parse_str(&shared_link.id)
            .map_err(|_| ErrorResp::Unauthorized("Invalid share key".to_string()))?;

        let mut allowed = HashSet::new();
        for album_id in album_ids {
            if album::shared_link_has_album(pool, &link_id, album_id).await? {
                allowed.insert(*album_id);
            }
        }
        return Ok(allowed);
    }

    let level = match permission {
        Permission::AlbumDelete => AlbumAccessLevel::Owner,
        Permission::AlbumUpdate
        | Permission::AlbumAddAsset
        | Permission::AlbumRemoveAsset
        | Permission::AlbumShare => AlbumAccessLevel::Editor,
        Permission::AlbumRead | Permission::AlbumDownload => AlbumAccessLevel::Member,
        _ => {
            return Err(ErrorResp::BadRequest(format!(
                "Unsupported album permission: {}",
                permission.as_str()
            )));
        }
    };

    let mut allowed = HashSet::new();
    for album_id in album_ids {
        if album::has_album_access(pool, &auth.user.id, album_id, level).await? {
            allowed.insert(*album_id);
        }
    }
    Ok(allowed)
}

pub fn require_upload_access(auth: &AuthDto) -> Result<(), ErrorResp> {
    if let Some(shared_link) = &auth.shared_link {
        if shared_link.allow_upload {
            Ok(())
        } else {
            Err(ErrorResp::Unauthorized("Unauthorized".to_string()))
        }
    } else {
        Ok(())
    }
}
