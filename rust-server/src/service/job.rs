use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

use bullmq_rs::JobOptions;
use crate::models::response::response::ErrorResp;

const BULL_PREFIX: &str = "immich_bull";

const QUEUE_BACKGROUND: &str = "backgroundTask";
const QUEUE_SIDECAR: &str = "sidecar";
const QUEUE_METADATA: &str = "metadataExtraction";
const QUEUE_THUMBNAIL: &str = "thumbnailGeneration";
const QUEUE_VIDEO: &str = "videoConversion";
const QUEUE_EDITOR: &str = "editor";
const QUEUE_NOTIFICATION: &str = "notifications";
const QUEUE_FACE: &str = "faceDetection";
const QUEUE_FACIAL: &str = "facialRecognition";
const QUEUE_SMART_SEARCH: &str = "smartSearch";
const QUEUE_DUPLICATE: &str = "duplicateDetection";
const QUEUE_STORAGE_TEMPLATE: &str = "storageTemplateMigration";
const QUEUE_MIGRATION: &str = "migration";
const QUEUE_LIBRARY: &str = "library";
const QUEUE_BACKUP: &str = "backupDatabase";
const QUEUE_OCR: &str = "ocr";
const QUEUE_INTEGRITY: &str = "integrityCheck";

pub const ALBUM_UPDATE_EMAIL_DELAY_MS: u64 = 300_000;

#[derive(Clone)]
pub struct JobService {
    redis_url: String,
}

#[derive(Serialize, Deserialize)]
struct EntityJob {
    id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notify: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct FileDeleteJob {
    files: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct EmptyTrashJob {}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyAlbumInviteJob {
    pub id: Uuid,
    pub recipient_id: Uuid,
    pub sender_name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyAlbumUpdateJob {
    pub id: Uuid,
    pub recipient_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyUserSignupJob {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMailJob {
    pub to: String,
    pub subject: String,
    pub html: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_attachments: Option<Vec<SendMailAttachmentJob>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMailAttachmentJob {
    pub filename: String,
    pub path: String,
    pub cid: String,
}

impl JobService {
    pub fn new(redis_url: String) -> Self {
        Self { redis_url }
    }

    /// Queue metadata extraction after a new asset upload.
    /// Node microservices will chain thumbnail generation, smart search, etc.
    pub async fn queue_asset_extract_metadata(&self, asset_id: &Uuid) -> Result<(), ErrorResp> {
        self.queue_asset_extract_metadata_with_source(asset_id, "upload")
            .await
    }

    pub async fn queue_asset_extract_metadata_all(&self, asset_ids: &[Uuid]) -> Result<(), ErrorResp> {
        for asset_id in asset_ids {
            self.queue_asset_extract_metadata(asset_id).await?;
        }
        Ok(())
    }

    pub async fn queue_asset_extract_metadata_with_source(
        &self,
        asset_id: &Uuid,
        source: &str,
    ) -> Result<(), ErrorResp> {
        self.add_job(
            QUEUE_METADATA,
            "AssetExtractMetadata",
            EntityJob {
                id: *asset_id,
                source: Some(source.to_string()),
                notify: None,
            },
        )
        .await
    }

    pub async fn queue_asset_generate_thumbnails(&self, asset_id: &Uuid) -> Result<(), ErrorResp> {
        self.add_job(
            QUEUE_THUMBNAIL,
            "AssetGenerateThumbnails",
            EntityJob {
                id: *asset_id,
                source: Some("upload".to_string()),
                notify: None,
            },
        )
        .await
    }

    pub async fn queue_asset_edit_thumbnails(&self, asset_id: &Uuid) -> Result<(), ErrorResp> {
        self.add_job(
            QUEUE_EDITOR,
            "AssetEditThumbnailGeneration",
            EntityJob {
                id: *asset_id,
                source: Some("upload".to_string()),
                notify: None,
            },
        )
        .await
    }

    pub async fn queue_asset_generate_thumbnails_with_notify(
        &self,
        asset_id: &Uuid,
        notify: bool,
    ) -> Result<(), ErrorResp> {
        self.add_job(
            QUEUE_THUMBNAIL,
            "AssetGenerateThumbnails",
            EntityJob {
                id: *asset_id,
                source: Some("upload".to_string()),
                notify: Some(notify),
            },
        )
        .await
    }

    pub async fn queue_asset_encode_video(&self, asset_id: &Uuid) -> Result<(), ErrorResp> {
        self.add_job(
            QUEUE_VIDEO,
            "AssetEncodeVideo",
            EntityJob {
                id: *asset_id,
                source: Some("upload".to_string()),
                notify: None,
            },
        )
        .await
    }

    pub async fn queue_sidecar_write(&self, asset_id: &Uuid) -> Result<(), ErrorResp> {
        self.queue_sidecar_write_all(&[*asset_id]).await
    }

    pub async fn queue_sidecar_write_all(&self, asset_ids: &[Uuid]) -> Result<(), ErrorResp> {
        if asset_ids.is_empty() {
            return Ok(());
        }

        for asset_id in asset_ids {
            self.add_job(
                QUEUE_SIDECAR,
                "SidecarWrite",
                EntityJob {
                    id: *asset_id,
                    source: None,
                    notify: None,
                },
            )
            .await?;
        }

        Ok(())
    }

    pub async fn queue_file_delete(&self, files: &[impl AsRef<str>]) -> Result<(), ErrorResp> {
        if files.is_empty() {
            return Ok(());
        }

        self.add_job(
            QUEUE_BACKGROUND,
            "FileDelete",
            FileDeleteJob {
                files: files.iter().map(|f| f.as_ref().to_string()).collect(),
            },
        )
        .await
    }

    pub async fn queue_asset_empty_trash(&self) -> Result<(), ErrorResp> {
        self.add_job(QUEUE_BACKGROUND, "AssetEmptyTrash", EmptyTrashJob {})
            .await
    }

    pub async fn queue_notify_album_invite(
        &self,
        album_id: &Uuid,
        recipient_id: &Uuid,
        sender_name: &str,
    ) -> Result<(), ErrorResp> {
        self.add_job(
            QUEUE_NOTIFICATION,
            "NotifyAlbumInvite",
            NotifyAlbumInviteJob {
                id: *album_id,
                recipient_id: *recipient_id,
                sender_name: sender_name.to_string(),
            },
        )
        .await
    }

    pub async fn queue_notify_album_update(
        &self,
        album_id: &Uuid,
        recipient_id: &Uuid,
    ) -> Result<(), ErrorResp> {
        self.remove_notification_job(&format!("{album_id}/{recipient_id}"))
            .await?;
        self.add_job_with_options(
            QUEUE_NOTIFICATION,
            "NotifyAlbumUpdate",
            NotifyAlbumUpdateJob {
                id: *album_id,
                recipient_id: *recipient_id,
                delay: Some(ALBUM_UPDATE_EMAIL_DELAY_MS),
            },
            Some(JobOptions {
                job_id: Some(format!("{album_id}/{recipient_id}")),
                delay: Some(Duration::from_millis(ALBUM_UPDATE_EMAIL_DELAY_MS)),
                ..Default::default()
            }),
        )
        .await
    }

    pub async fn queue_notify_user_signup(
        &self,
        user_id: &Uuid,
        password: Option<String>,
    ) -> Result<(), ErrorResp> {
        self.add_job(
            QUEUE_NOTIFICATION,
            "NotifyUserSignup",
            NotifyUserSignupJob {
                id: *user_id,
                password,
            },
        )
        .await
    }

    pub async fn queue_send_mail(&self, job: SendMailJob) -> Result<(), ErrorResp> {
        self.add_job(QUEUE_NOTIFICATION, "SendMail", job).await
    }

    pub async fn remove_notification_job(&self, job_id: &str) -> Result<(), ErrorResp> {
        let queue = bullmq_rs::QueueBuilder::new(QUEUE_NOTIFICATION)
            .prefix(BULL_PREFIX)
            .connection(bullmq_rs::RedisConnection::new(self.redis_url.clone()))
            .build::<serde_json::Value>()
            .await
            .map_err(|err| ErrorResp::ServerError(format!("Failed to init notification queue: {err}")))?;

        if queue.get_job(job_id).await.ok().flatten().is_some() {
            queue.remove(job_id).await.map_err(|err| {
                ErrorResp::ServerError(format!("Failed to remove notification job: {err}"))
            })?;
        }
        Ok(())
    }

    pub async fn queue_json_job(
        &self,
        queue_name: &str,
        job_name: &str,
        data: serde_json::Value,
    ) -> Result<(), ErrorResp> {
        self.add_job(queue_name, job_name, data).await
    }

    pub async fn queue_json_job_empty(
        &self,
        queue_name: &str,
        job_name: &str,
    ) -> Result<(), ErrorResp> {
        self.add_job(queue_name, job_name, serde_json::json!({})).await
    }

    pub async fn create_manual_job(&self, name: &str) -> Result<(), ErrorResp> {
        match name {
            "tag-cleanup" => self.queue_json_job_empty(QUEUE_BACKGROUND, "TagCleanup").await,
            "person-cleanup" => {
                self.queue_json_job_empty(QUEUE_BACKGROUND, "PersonCleanup")
                    .await
            }
            "user-cleanup" => {
                self.queue_json_job_empty(QUEUE_BACKGROUND, "UserDeleteCheck")
                    .await
            }
            "memory-cleanup" => {
                self.queue_json_job_empty(QUEUE_BACKGROUND, "MemoryCleanup")
                    .await
            }
            "memory-create" => {
                self.queue_json_job_empty(QUEUE_BACKGROUND, "MemoryGenerate")
                    .await
            }
            "backup-database" => {
                self.queue_json_job(QUEUE_BACKUP, "DatabaseBackup", serde_json::json!({}))
                    .await
            }
            "integrity-missing-files" => {
                self.queue_json_job(
                    QUEUE_INTEGRITY,
                    "IntegrityMissingFilesQueueAll",
                    serde_json::json!({}),
                )
                .await
            }
            "integrity-untracked-files" => {
                self.queue_json_job(
                    QUEUE_INTEGRITY,
                    "IntegrityUntrackedFilesQueueAll",
                    serde_json::json!({}),
                )
                .await
            }
            "integrity-checksum-mismatch" => {
                self.queue_json_job(
                    QUEUE_INTEGRITY,
                    "IntegrityChecksumFiles",
                    serde_json::json!({}),
                )
                .await
            }
            "integrity-missing-files-refresh" => {
                self.queue_json_job(
                    QUEUE_INTEGRITY,
                    "IntegrityMissingFilesQueueAll",
                    serde_json::json!({ "refreshOnly": true }),
                )
                .await
            }
            "integrity-untracked-files-refresh" => {
                self.queue_json_job(
                    QUEUE_INTEGRITY,
                    "IntegrityUntrackedFilesQueueAll",
                    serde_json::json!({ "refreshOnly": true }),
                )
                .await
            }
            "integrity-checksum-mismatch-refresh" => {
                self.queue_json_job(
                    QUEUE_INTEGRITY,
                    "IntegrityChecksumFiles",
                    serde_json::json!({ "refreshOnly": true }),
                )
                .await
            }
            "integrity-missing-files-delete-all" => {
                self.queue_json_job(
                    QUEUE_INTEGRITY,
                    "IntegrityDeleteReportType",
                    serde_json::json!({ "type": "missing-file" }),
                )
                .await
            }
            "integrity-untracked-files-delete-all" => {
                self.queue_json_job(
                    QUEUE_INTEGRITY,
                    "IntegrityDeleteReportType",
                    serde_json::json!({ "type": "untracked-file" }),
                )
                .await
            }
            "integrity-checksum-mismatch-delete-all" => {
                self.queue_json_job(
                    QUEUE_INTEGRITY,
                    "IntegrityDeleteReportType",
                    serde_json::json!({ "type": "checksum-fail" }),
                )
                .await
            }
            _ => Err(ErrorResp::BadRequest("Invalid job name".to_string())),
        }
    }

    pub async fn run_asset_jobs(
        &self,
        job_name: &str,
        asset_ids: &[Uuid],
    ) -> Result<(), ErrorResp> {
        let (queue, bull_job) = match job_name {
            "refresh-faces" => (QUEUE_FACE, "AssetDetectFaces"),
            "refresh-metadata" => (QUEUE_METADATA, "AssetExtractMetadata"),
            "regenerate-thumbnail" => (QUEUE_THUMBNAIL, "AssetGenerateThumbnails"),
            "transcode-video" => (QUEUE_VIDEO, "AssetEncodeVideo"),
            _ => return Err(ErrorResp::BadRequest("Invalid asset job name".to_string())),
        };

        for asset_id in asset_ids {
            self.add_job(
                queue,
                bull_job,
                EntityJob {
                    id: *asset_id,
                    source: None,
                    notify: None,
                },
            )
            .await?;
        }
        Ok(())
    }

    pub async fn queue_library_scan(&self, library_id: &Uuid) -> Result<(), ErrorResp> {
        let data = serde_json::json!({ "id": library_id });
        self.queue_json_job(QUEUE_LIBRARY, "LibrarySyncFilesQueueAll", data.clone())
            .await?;
        self.queue_json_job(QUEUE_LIBRARY, "LibraryScanAssetsQueueAll", data)
            .await
    }

    pub async fn queue_library_delete(&self, library_id: &Uuid) -> Result<(), ErrorResp> {
        self.queue_json_job(
            QUEUE_LIBRARY,
            "LibraryDelete",
            serde_json::json!({ "id": library_id }),
        )
        .await
    }

    pub async fn queue_start_command(
        &self,
        queue_name: &str,
        force: bool,
    ) -> Result<(), ErrorResp> {
        let queue = self.json_queue(queue_name).await?;
        if queue.get_active_count().await.unwrap_or(0) > 0 {
            return Err(ErrorResp::BadRequest("Job is already running".to_string()));
        }

        let force_value = serde_json::json!({ "force": force });

        match queue_name {
            "videoConversion" => {
                self.queue_json_job(QUEUE_VIDEO, "AssetEncodeVideoQueueAll", force_value)
                    .await
            }
            "storageTemplateMigration" => {
                self.queue_json_job_empty(QUEUE_STORAGE_TEMPLATE, "StorageTemplateMigration")
                    .await
            }
            "migration" => {
                self.queue_json_job_empty(QUEUE_MIGRATION, "FileMigrationQueueAll")
                    .await
            }
            "smartSearch" => {
                self.queue_json_job(QUEUE_SMART_SEARCH, "SmartSearchQueueAll", force_value)
                    .await
            }
            "duplicateDetection" => {
                self.queue_json_job(
                    QUEUE_DUPLICATE,
                    "AssetDetectDuplicatesQueueAll",
                    force_value,
                )
                .await
            }
            "metadataExtraction" => {
                self.queue_json_job(
                    QUEUE_METADATA,
                    "AssetExtractMetadataQueueAll",
                    force_value,
                )
                .await
            }
            "sidecar" => {
                self.queue_json_job(QUEUE_SIDECAR, "SidecarQueueAll", force_value)
                    .await
            }
            "thumbnailGeneration" => {
                self.queue_json_job(
                    QUEUE_THUMBNAIL,
                    "AssetGenerateThumbnailsQueueAll",
                    force_value,
                )
                .await
            }
            "faceDetection" => {
                self.queue_json_job(QUEUE_FACE, "AssetDetectFacesQueueAll", force_value)
                    .await
            }
            "facialRecognition" => {
                self.queue_json_job(
                    QUEUE_FACIAL,
                    "FacialRecognitionQueueAll",
                    force_value,
                )
                .await
            }
            "library" => {
                self.queue_json_job(QUEUE_LIBRARY, "LibraryScanQueueAll", force_value)
                    .await
            }
            "backupDatabase" => {
                self.queue_json_job(QUEUE_BACKUP, "DatabaseBackup", force_value)
                    .await
            }
            "ocr" => {
                self.queue_json_job(QUEUE_OCR, "OcrQueueAll", force_value).await
            }
            _ => Err(ErrorResp::BadRequest(format!("Invalid job name: {queue_name}"))),
        }
    }

    pub(crate) async fn json_queue(
        &self,
        queue_name: &str,
    ) -> Result<bullmq_rs::Queue<serde_json::Value>, ErrorResp> {
        bullmq_rs::QueueBuilder::new(queue_name)
            .prefix(BULL_PREFIX)
            .connection(bullmq_rs::RedisConnection::new(self.redis_url.clone()))
            .build::<serde_json::Value>()
            .await
            .map_err(|err| {
                ErrorResp::ServerError(format!("Failed to init job queue '{queue_name}': {err}"))
            })
    }

    async fn add_job_with_options<T>(
        &self,
        queue_name: &str,
        job_name: &str,
        data: T,
        opts: Option<JobOptions>,
    ) -> Result<(), ErrorResp>
    where
        T: Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    {
        let queue = bullmq_rs::QueueBuilder::new(queue_name)
            .prefix(BULL_PREFIX)
            .connection(bullmq_rs::RedisConnection::new(self.redis_url.clone()))
            .build::<T>()
            .await
            .map_err(|err| {
                ErrorResp::ServerError(format!("Failed to init job queue '{queue_name}': {err}"))
            })?;

        queue.add(job_name, data, opts).await.map_err(|err| {
            ErrorResp::ServerError(format!(
                "Failed to queue {job_name} on '{queue_name}': {err}"
            ))
        })?;

        Ok(())
    }

    async fn add_job<T>(
        &self,
        queue_name: &str,
        job_name: &str,
        data: T,
    ) -> Result<(), ErrorResp>
    where
        T: Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    {
        let queue = bullmq_rs::QueueBuilder::new(queue_name)
            .prefix(BULL_PREFIX)
            .connection(bullmq_rs::RedisConnection::new(self.redis_url.clone()))
            .build::<T>()
            .await
            .map_err(|err| {
                ErrorResp::ServerError(format!("Failed to init job queue '{queue_name}': {err}"))
            })?;

        queue.add(job_name, data, None).await.map_err(|err| {
            ErrorResp::ServerError(format!(
                "Failed to queue {job_name} on '{queue_name}': {err}"
            ))
        })?;

        Ok(())
    }
}
