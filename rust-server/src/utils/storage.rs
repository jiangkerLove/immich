use std::path::{Path, PathBuf};

use uuid::Uuid;

#[derive(Clone)]
pub struct StoragePaths {
    media_location: PathBuf,
}

impl StoragePaths {
    pub fn new(media_location: impl Into<PathBuf>) -> Self {
        Self {
            media_location: media_location.into(),
        }
    }

    pub fn media_location(&self) -> &Path {
        &self.media_location
    }

    pub fn upload_folder(&self, owner_id: &Uuid, file_uuid: &str) -> PathBuf {
        let base = self
            .media_location
            .join("upload")
            .join(owner_id.to_string());
        base.join(&file_uuid[..2]).join(&file_uuid[2..4])
    }

    pub fn upload_path(&self, owner_id: &Uuid, file_uuid: &str, filename: &str) -> PathBuf {
        self.upload_folder(owner_id, file_uuid).join(filename)
    }

    pub fn thumbnail_path(&self, owner_id: &Uuid, asset_id: &Uuid, suffix: &str) -> PathBuf {
        self.media_location
            .join("thumbs")
            .join(owner_id.to_string())
            .join(format!("{asset_id}_{suffix}"))
    }

    pub fn profile_image_path(&self, user_id: &Uuid, file_id: &str, extension: &str) -> PathBuf {
        self.media_location
            .join("profile")
            .join(user_id.to_string())
            .join(format!("{file_id}.{extension}"))
    }

    pub fn backups_folder(&self) -> PathBuf {
        self.media_location.join("backups")
    }

    pub fn image_derivative_path(
        &self,
        owner_id: &Uuid,
        asset_id: &Uuid,
        file_type: &str,
        format: &str,
        is_edited: bool,
    ) -> PathBuf {
        let edited = if is_edited { "_edited" } else { "" };
        let filename = format!("{asset_id}_{file_type}{edited}.{format}");
        Self::nested_file_path(&self.thumbs_base(), owner_id, &filename)
    }

    pub fn person_thumbnail_path(&self, owner_id: &Uuid, person_id: &Uuid) -> PathBuf {
        let filename = format!("{person_id}.jpeg");
        Self::nested_file_path(&self.thumbs_base(), owner_id, &filename)
    }

    pub fn encoded_video_path(&self, owner_id: &Uuid, asset_id: &Uuid) -> PathBuf {
        let filename = format!("{asset_id}.mp4");
        Self::nested_file_path(&self.encoded_video_base(), owner_id, &filename)
    }

    pub fn android_motion_path(&self, owner_id: &Uuid, motion_id: &Uuid) -> PathBuf {
        let filename = format!("{motion_id}-MP.mp4");
        Self::nested_file_path(&self.encoded_video_base(), owner_id, &filename)
    }

    pub fn nested_file_path(base: &Path, owner_id: &Uuid, filename: &str) -> PathBuf {
        let prefix2 = filename.get(0..2).unwrap_or("00");
        let prefix4 = filename.get(2..4).unwrap_or("00");
        base.join(owner_id.to_string())
            .join(prefix2)
            .join(prefix4)
            .join(filename)
    }

    pub fn library_folder(&self, owner_id: &Uuid, storage_label: Option<&str>) -> PathBuf {
        let owner_id_str = owner_id.to_string();
        let folder = storage_label.unwrap_or(&owner_id_str);
        self.media_location.join("library").join(folder)
    }

    pub fn encoded_video_base(&self) -> PathBuf {
        self.media_location.join("encoded-video")
    }

    pub fn upload_base(&self) -> PathBuf {
        self.media_location.join("upload")
    }

    pub fn library_base(&self) -> PathBuf {
        self.media_location.join("library")
    }

    pub fn profile_base(&self) -> PathBuf {
        self.media_location.join("profile")
    }

    pub fn thumbs_base(&self) -> PathBuf {
        self.media_location.join("thumbs")
    }

    pub fn user_upload_folder(&self, owner_id: &Uuid) -> PathBuf {
        self.upload_base().join(owner_id.to_string())
    }

    pub fn user_profile_folder(&self, owner_id: &Uuid) -> PathBuf {
        self.profile_base().join(owner_id.to_string())
    }

    pub fn user_thumbs_folder(&self, owner_id: &Uuid) -> PathBuf {
        self.thumbs_base().join(owner_id.to_string())
    }

    pub fn user_encoded_video_folder(&self, owner_id: &Uuid) -> PathBuf {
        self.encoded_video_base().join(owner_id.to_string())
    }

    pub fn hls_session_folder(&self, owner_id: &Uuid, session_id: &Uuid) -> PathBuf {
        Self::nested_file_path(
            &self.encoded_video_base(),
            owner_id,
            &session_id.to_string(),
        )
    }

    pub fn hls_variant_folder(
        &self,
        owner_id: &Uuid,
        session_id: &Uuid,
        variant_index: u32,
    ) -> PathBuf {
        self.hls_session_folder(owner_id, session_id)
            .join(variant_index.to_string())
    }

    pub fn is_immich_path(&self, path: &str) -> bool {
        let media = self.media_location.canonicalize().ok();
        let target = std::path::Path::new(path).canonicalize().ok();
        match (media, target) {
            (Some(media), Some(target)) => target.starts_with(&media),
            _ => {
                let normalized_media = format!("{}/", self.media_location.display());
                path.starts_with(&normalized_media)
                    || path.starts_with(self.media_location.to_string_lossy().as_ref())
            }
        }
    }

    pub fn ensure_parent(path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    pub async fn remove_empty_dirs(directory: &Path, remove_self: bool) -> Result<(), String> {
        remove_empty_dirs_inner(directory, remove_self).await
    }
}

async fn remove_empty_dirs_inner(directory: &Path, remove_self: bool) -> Result<(), String> {
    let metadata = tokio::fs::symlink_metadata(directory)
        .await
        .map_err(|err| err.to_string())?;
    if !metadata.is_dir() {
        return Ok(());
    }

    let mut entries = tokio::fs::read_dir(directory)
        .await
        .map_err(|err| err.to_string())?;
    while let Some(entry) = entries.next_entry().await.map_err(|err| err.to_string())? {
        Box::pin(remove_empty_dirs_inner(&entry.path(), true)).await?;
    }

    if remove_self {
        let mut remaining = tokio::fs::read_dir(directory)
            .await
            .map_err(|err| err.to_string())?;
        if remaining
            .next_entry()
            .await
            .map_err(|err| err.to_string())?
            .is_none()
        {
            if let Err(err) = tokio::fs::remove_dir(directory).await {
                if !matches!(err.raw_os_error(), Some(39) | Some(66)) {
                    eprintln!("attempted to remove directory {directory:?}, but failed: {err}");
                }
            }
        }
    }

    Ok(())
}
