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
}
