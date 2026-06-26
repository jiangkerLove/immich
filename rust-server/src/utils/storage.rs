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
        let suffix = if is_edited { "_edited" } else { "" };
        self.media_location
            .join("thumbs")
            .join(owner_id.to_string())
            .join(format!("{asset_id}_{file_type}{suffix}.{format}"))
    }

    pub fn person_thumbnail_path(&self, owner_id: &Uuid, person_id: &Uuid) -> PathBuf {
        self.media_location
            .join("thumbs")
            .join(owner_id.to_string())
            .join(format!("{person_id}.jpeg"))
    }

    pub fn encoded_video_path(&self, owner_id: &Uuid, asset_id: &Uuid) -> PathBuf {
        self.media_location
            .join("encoded-video")
            .join(owner_id.to_string())
            .join(format!("{asset_id}.mp4"))
    }

    pub fn library_folder(&self, owner_id: &Uuid, storage_label: Option<&str>) -> PathBuf {
        let owner_id_str = owner_id.to_string();
        let folder = storage_label.unwrap_or(&owner_id_str);
        self.media_location.join("library").join(folder)
    }

    pub fn encoded_video_base(&self) -> PathBuf {
        self.media_location.join("encoded-video")
    }

    pub fn ensure_parent(path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}
