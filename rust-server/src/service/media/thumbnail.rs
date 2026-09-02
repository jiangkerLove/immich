use std::path::Path;
use std::process::Stdio;

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat};
use serde_json::Value;
use sqlx::PgPool;
use thumbhash::rgba_to_thumb_hash;
use tokio::process::Command;
use uuid::Uuid;

use crate::models::db::asset_edit::AssetEditRow;
use crate::models::db::asset_job::{self, ThumbnailAssetJob, UpsertAssetFile};
use crate::models::db::asset_ocr;
use crate::models::db::face;
use crate::models::db::system_metadata::get_json;
use crate::service::job::{EntityJob, JobService};
use crate::service::media::edits::{
    apply_edits, apply_exif_orientation, face_crop_from_bbox, output_dimensions, parse_crop,
};
use crate::service::media::exiftool;
use crate::service::media::ffmpeg_tonemap::{
    VideoThumbnailStream, append_video_thumbnail_input_args, build_video_thumbnail_vf,
};
use crate::service::media::visibility::{
    BoundingBox, FaceForVisibility, OcrForVisibility, asset_dimensions_from_exif,
    check_face_visibility, check_ocr_visibility, visible_ocr_search_text,
};
use crate::utils::storage::StoragePaths;
use crate::utils::system_config::json_str;

const FACE_THUMBNAIL_SIZE: u32 = 250;
const JOBS_BATCH_SIZE: usize = 1000;

const RAW_EXTENSIONS: &[&str] = &[
    ".3fr", ".ari", ".arw", ".cap", ".cin", ".cr2", ".cr3", ".crw", ".dcr", ".dng", ".erf", ".fff",
    ".iiq", ".k25", ".kdc", ".mrw", ".nef", ".nrw", ".orf", ".ori", ".pef", ".psd", ".raf", ".raw",
    ".rw2", ".rwl", ".sr2", ".srf", ".srw", ".x3f",
];

const WEB_UNSUPPORTED_EXTENSIONS: &[&str] = &[
    ".3fr", ".ari", ".arw", ".cap", ".cin", ".cr2", ".cr3", ".crw", ".dcr", ".dng", ".erf", ".fff",
    ".iiq", ".k25", ".kdc", ".mrw", ".nef", ".nrw", ".orf", ".ori", ".pef", ".psd", ".raf", ".raw",
    ".rw2", ".rwl", ".sr2", ".srf", ".srw", ".x3f", ".heic", ".heif", ".hif", ".insp", ".jp2",
    ".jpe", ".jxl", ".mpo", ".svg", ".tif", ".tiff",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailJobOutcome {
    Success,
    Skipped,
    Failed,
}

#[derive(Debug, Clone)]
struct ImageFormatConfig {
    preview_format: String,
    preview_size: u32,
    preview_quality: u8,
    thumbnail_format: String,
    thumbnail_size: u32,
    thumbnail_quality: u8,
    fullsize_enabled: bool,
    fullsize_format: String,
    fullsize_quality: u8,
    extract_embedded: bool,
}

impl Default for ImageFormatConfig {
    fn default() -> Self {
        Self {
            preview_format: "jpeg".into(),
            preview_size: 1440,
            thumbnail_format: "webp".into(),
            thumbnail_size: 250,
            preview_quality: 80,
            thumbnail_quality: 80,
            fullsize_enabled: false,
            fullsize_format: "jpeg".into(),
            fullsize_quality: 80,
            extract_embedded: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct GeneratedOutputs {
    thumbhash: Option<Vec<u8>>,
    width: Option<i32>,
    height: Option<i32>,
}

#[derive(Clone)]
pub struct ThumbnailService {
    pool: PgPool,
    storage: StoragePaths,
    jobs: JobService,
}

impl ThumbnailService {
    pub fn new(pool: PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self {
            pool,
            storage,
            jobs,
        }
    }

    pub async fn generate_asset_thumbnails(
        &self,
        asset_id: &Uuid,
        job: &EntityJob,
    ) -> Result<ThumbnailJobOutcome, String> {
        let Some(asset) = asset_job::get_for_generate_thumbnail(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            eprintln!("thumbnail generation failed for {asset_id}: missing asset or metadata");
            return Ok(ThumbnailJobOutcome::Failed);
        };

        if asset.visibility == "hidden" {
            return Ok(ThumbnailJobOutcome::Skipped);
        }

        let config = self.load_image_config().await?;

        let is_gif = asset
            .original_file_name
            .to_ascii_lowercase()
            .ends_with(".gif");
        let generated = if asset.asset_type == "VIDEO" || is_gif {
            self.generate_video_like(&asset, &config, false).await?
        } else if asset.asset_type == "IMAGE" {
            self.generate_image(&asset, &config, false).await?
        } else {
            eprintln!(
                "skipping thumbnail generation for {}: type {} is not image/video",
                asset.id, asset.asset_type
            );
            return Ok(ThumbnailJobOutcome::Skipped);
        };

        if generated == ThumbnailJobOutcome::Failed {
            return Ok(ThumbnailJobOutcome::Failed);
        }

        let edited = self
            .generate_edited_image_derivatives(&asset, &config)
            .await?;
        if let Some(edited_outputs) = edited {
            if let Some(hash) = edited_outputs.thumbhash.as_ref() {
                asset_job::update_thumbhash(&self.pool, asset_id, hash)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        self.sync_post_edit_visibility(&asset).await?;

        if generated == ThumbnailJobOutcome::Success {
            asset_job::update_job_status_thumbnails(&self.pool, asset_id)
                .await
                .map_err(|err| err.to_string())?;
            self.queue_follow_up_jobs(job, asset.asset_type == "VIDEO")
                .await?;
        }

        Ok(generated)
    }

    pub async fn generate_asset_edit_thumbnails(
        &self,
        asset_id: &Uuid,
    ) -> Result<ThumbnailJobOutcome, String> {
        let Some(asset) = asset_job::get_for_generate_thumbnail(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            eprintln!("edit thumbnail generation failed for {asset_id}: missing asset or metadata");
            return Ok(ThumbnailJobOutcome::Failed);
        };

        let config = self.load_image_config().await?;
        let generated = self
            .generate_edited_image_derivatives(&asset, &config)
            .await?;

        let mut thumbhash = generated.as_ref().and_then(|g| g.thumbhash.clone());

        if thumbhash.is_none() {
            if let Ok((image, _, _)) = self.decode_asset_image(&asset, &config, false).await {
                let oriented = apply_exif_orientation(image, asset.orientation.as_deref());
                thumbhash = Some(compute_thumbhash_from_image(&oriented)?);
            }
        }

        if let Some(hash) = thumbhash.as_ref() {
            if asset
                .thumbhash
                .as_ref()
                .map(|existing| existing.as_slice() != hash.as_slice())
                .unwrap_or(true)
            {
                asset_job::update_thumbhash(&self.pool, asset_id, hash)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        let (width, height) = if let Some(outputs) = &generated {
            (outputs.width.unwrap_or(0), outputs.height.unwrap_or(0))
        } else {
            (
                asset.exif_image_width.unwrap_or(0),
                asset.exif_image_height.unwrap_or(0),
            )
        };

        if width > 0 && height > 0 {
            asset_job::update_asset_dimensions(&self.pool, asset_id, width, height)
                .await
                .map_err(|err| err.to_string())?;
        }

        self.sync_post_edit_visibility(&asset).await?;

        if generated.is_some() {
            Ok(ThumbnailJobOutcome::Success)
        } else {
            Ok(ThumbnailJobOutcome::Success)
        }
    }

    pub async fn generate_person_thumbnail(
        &self,
        person_id: &Uuid,
    ) -> Result<ThumbnailJobOutcome, String> {
        if !self.is_person_thumbnail_enabled().await? {
            return Ok(ThumbnailJobOutcome::Skipped);
        }

        let Some(data) = asset_job::get_person_thumbnail_job_data(&self.pool, person_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            eprintln!("person thumbnail generation failed for {person_id}: missing data");
            return Ok(ThumbnailJobOutcome::Failed);
        };

        let input_path = if data.asset_type == "VIDEO" {
            let Some(preview) = data.preview_path.as_ref() else {
                eprintln!(
                    "person thumbnail generation failed for {person_id}: missing video preview"
                );
                return Ok(ThumbnailJobOutcome::Failed);
            };
            preview.clone()
        } else {
            data.original_path.clone()
        };

        if !Path::new(&input_path).exists() {
            return Ok(ThumbnailJobOutcome::Failed);
        }

        let config = self.load_image_config().await?;
        let mut skip_orientation = false;
        let decoded = if data.asset_type != "VIDEO"
            && config.extract_embedded
            && is_raw_file(&data.original_path)
        {
            if let Some(preview) =
                extract_raw_embedded_preview(&data.original_path, config.preview_size).await?
            {
                skip_orientation = true;
                decode_image_bytes(&preview).await?
            } else {
                extract_with_ffmpeg(&data.original_path, config.preview_size).await?
            }
        } else {
            match decode_image_path(&input_path).await {
                Ok(image) => image,
                Err(err) => {
                    eprintln!("person thumbnail decode failed for {person_id}: {err}");
                    extract_with_ffmpeg(&input_path, config.preview_size).await?
                }
            }
        };

        let oriented = if skip_orientation {
            decoded
        } else {
            apply_exif_orientation(decoded, data.exif_orientation.as_deref())
        };
        let (width, height) = oriented.dimensions();
        let crop = face_crop_from_bbox(
            data.old_width,
            data.old_height,
            width,
            height,
            data.x1,
            data.y1,
            data.x2,
            data.y2,
        );
        let crop_edit = AssetEditRow {
            id: Uuid::nil(),
            action: "crop".into(),
            parameters: serde_json::json!({
                "x": crop.x,
                "y": crop.y,
                "width": crop.width,
                "height": crop.height,
            }),
        };
        let cropped = apply_edits(oriented, &[crop_edit]);

        let thumbnail_path = self
            .storage
            .person_thumbnail_path(&data.owner_id, person_id);
        write_resized(
            &cropped,
            &thumbnail_path,
            FACE_THUMBNAIL_SIZE,
            "jpeg",
            config.thumbnail_quality,
        )?;

        asset_job::update_person_thumbnail_path(
            &self.pool,
            person_id,
            &thumbnail_path.to_string_lossy(),
        )
        .await
        .map_err(|err| err.to_string())?;

        Ok(ThumbnailJobOutcome::Success)
    }

    pub async fn queue_all_thumbnails(&self, force: bool) -> Result<(), String> {
        let config = self.load_image_config().await?;
        let assets =
            asset_job::stream_for_thumbnail_job(&self.pool, force, config.fullsize_enabled)
                .await
                .map_err(|err| err.to_string())?;

        let mut batch: Vec<(String, EntityJob)> = Vec::new();
        for asset in assets {
            if force || !asset.is_edited {
                batch.push((
                    "AssetGenerateThumbnails".into(),
                    EntityJob {
                        id: asset.id,
                        source: None,
                        notify: None,
                    },
                ));
            }
            if asset.is_edited {
                batch.push((
                    "AssetEditThumbnailGeneration".into(),
                    EntityJob {
                        id: asset.id,
                        source: None,
                        notify: None,
                    },
                ));
            }
            if batch.len() >= JOBS_BATCH_SIZE {
                self.flush_thumbnail_queue_batch(&batch).await?;
                batch.clear();
            }
        }
        self.flush_thumbnail_queue_batch(&batch).await?;

        let people = asset_job::stream_people_for_thumbnail_job(&self.pool, force)
            .await
            .map_err(|err| err.to_string())?;
        batch.clear();
        for person in people {
            if person.face_asset_id.is_none() {
                if let Some(face_id) = asset_job::get_random_face_id(&self.pool, &person.id)
                    .await
                    .map_err(|err| err.to_string())?
                {
                    asset_job::update_person_face_asset_id(&self.pool, &person.id, &face_id)
                        .await
                        .map_err(|err| err.to_string())?;
                } else {
                    continue;
                }
            }

            batch.push((
                "PersonGenerateThumbnail".into(),
                EntityJob {
                    id: person.id,
                    source: None,
                    notify: None,
                },
            ));
            if batch.len() >= JOBS_BATCH_SIZE {
                self.flush_person_queue_batch(&batch).await?;
                batch.clear();
            }
        }
        self.flush_person_queue_batch(&batch).await?;

        Ok(())
    }

    async fn flush_thumbnail_queue_batch(
        &self,
        batch: &[(String, EntityJob)],
    ) -> Result<(), String> {
        for (name, job) in batch {
            match name.as_str() {
                "AssetGenerateThumbnails" => {
                    self.jobs
                        .queue_asset_generate_thumbnails(&job.id)
                        .await
                        .map_err(|err| err.to_string())?;
                }
                "AssetEditThumbnailGeneration" => {
                    self.jobs
                        .queue_asset_edit_thumbnails(&job.id)
                        .await
                        .map_err(|err| err.to_string())?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn flush_person_queue_batch(&self, batch: &[(String, EntityJob)]) -> Result<(), String> {
        for (_, job) in batch {
            self.jobs
                .queue_person_generate_thumbnail(&job.id)
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    async fn sync_post_edit_visibility(&self, asset: &ThumbnailAssetJob) -> Result<(), String> {
        if asset.asset_type != "IMAGE" {
            return Ok(());
        }
        if asset.files.is_empty() && asset.edits.is_empty() {
            return Ok(());
        }

        let crop_box = crop_box_from_edits(&asset.edits);
        let dimensions = asset_dimensions_from_exif(
            asset.exif_image_width,
            asset.exif_image_height,
            asset.orientation.as_deref(),
        );

        let face_rows = face::list_for_visibility_by_asset(&self.pool, &asset.id)
            .await
            .map_err(|err| err.to_string())?;
        let faces: Vec<FaceForVisibility> = face_rows
            .into_iter()
            .map(|row| FaceForVisibility {
                id: row.id,
                bounding_box_x1: row.bounding_box_x1,
                bounding_box_y1: row.bounding_box_y1,
                bounding_box_x2: row.bounding_box_x2,
                bounding_box_y2: row.bounding_box_y2,
                image_width: row.image_width,
                image_height: row.image_height,
                is_visible: row.is_visible,
            })
            .collect();

        let face_update = check_face_visibility(&faces, dimensions, crop_box.as_ref());
        face::update_visibilities(
            &self.pool,
            &face_update.visible_ids,
            &face_update.hidden_ids,
        )
        .await
        .map_err(|err| err.to_string())?;

        let ocr_rows = asset_ocr::list_for_visibility_by_asset(&self.pool, &asset.id)
            .await
            .map_err(|err| err.to_string())?;
        let ocrs: Vec<OcrForVisibility> = ocr_rows
            .into_iter()
            .map(|row| OcrForVisibility {
                id: row.id,
                x1: row.x1,
                y1: row.y1,
                x2: row.x2,
                y2: row.y2,
                x3: row.x3,
                y3: row.y3,
                x4: row.x4,
                y4: row.y4,
                text: row.text,
                is_visible: row.is_visible,
            })
            .collect();

        let ocr_update = check_ocr_visibility(&ocrs, dimensions, crop_box.as_ref());
        let search_text = visible_ocr_search_text(&ocrs, &ocr_update.visible_ids);
        asset_ocr::update_visibilities(
            &self.pool,
            &asset.id,
            &ocr_update.visible_ids,
            &ocr_update.hidden_ids,
            &search_text,
        )
        .await
        .map_err(|err| err.to_string())?;

        Ok(())
    }

    async fn queue_follow_up_jobs(&self, job: &EntityJob, is_video: bool) -> Result<(), String> {
        if !job.notify.unwrap_or(false) && job.source.as_deref() != Some("upload") {
            return Ok(());
        }

        self.jobs
            .queue_post_thumbnail_ml_jobs(job, is_video)
            .await
            .map_err(|err| err.to_string())
    }

    async fn is_person_thumbnail_enabled(&self) -> Result<bool, String> {
        let stored = get_json(&self.pool, "system-config")
            .await
            .map_err(|err| err.to_string())?;
        let Some(config) = stored else {
            return Ok(false);
        };

        let ml_enabled = config
            .get("machineLearning")
            .and_then(|ml| ml.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let facial_enabled = config
            .get("machineLearning")
            .and_then(|ml| ml.get("facialRecognition"))
            .and_then(|fr| fr.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let face_import = config
            .get("metadata")
            .and_then(|md| md.get("faces"))
            .and_then(|faces| faces.get("import"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok((ml_enabled && facial_enabled) || face_import)
    }

    async fn load_image_config(&self) -> Result<ImageFormatConfig, String> {
        let mut config = ImageFormatConfig::default();
        let stored = get_json(&self.pool, "system-config")
            .await
            .map_err(|err| err.to_string())?;
        if let Some(image) = stored.and_then(|value| value.get("image").cloned()) {
            if let Some(preview) = image.get("preview") {
                config.preview_format = read_string(preview, "format", &config.preview_format);
                config.preview_size = read_u32(preview, "size", config.preview_size);
                config.preview_quality =
                    read_u32(preview, "quality", config.preview_quality as u32) as u8;
            }
            if let Some(thumbnail) = image.get("thumbnail") {
                config.thumbnail_format =
                    read_string(thumbnail, "format", &config.thumbnail_format);
                config.thumbnail_size = read_u32(thumbnail, "size", config.thumbnail_size);
                config.thumbnail_quality =
                    read_u32(thumbnail, "quality", config.thumbnail_quality as u32) as u8;
            }
            if let Some(fullsize) = image.get("fullsize") {
                config.fullsize_enabled = fullsize
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                config.fullsize_format = read_string(fullsize, "format", &config.fullsize_format);
                config.fullsize_quality =
                    read_u32(fullsize, "quality", config.fullsize_quality as u32) as u8;
            }
            config.extract_embedded = image
                .get("extractEmbedded")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        }
        Ok(config)
    }

    async fn load_ffmpeg_tonemap(&self) -> Result<String, String> {
        let stored = get_json(&self.pool, "system-config")
            .await
            .map_err(|err| err.to_string())?;
        let ffmpeg = stored
            .and_then(|value| value.get("ffmpeg").cloned())
            .unwrap_or_default();
        Ok(json_str(&ffmpeg, &["tonemap"], "hable"))
    }

    async fn generate_edited_image_derivatives(
        &self,
        asset: &ThumbnailAssetJob,
        config: &ImageFormatConfig,
    ) -> Result<Option<GeneratedOutputs>, String> {
        if asset.asset_type != "IMAGE" || asset.edits.is_empty() {
            return Ok(None);
        }
        if self.generate_image(asset, config, true).await? == ThumbnailJobOutcome::Failed {
            return Ok(None);
        }

        let thumbnail_path = self.storage.image_derivative_path(
            &asset.owner_id,
            &asset.id,
            "thumbnail",
            &config.thumbnail_format,
            true,
        );

        let (width, height) = match self.decode_asset_image(asset, config, true).await {
            Ok((_, w, h)) => output_dimensions(&asset.edits, w, h),
            Err(_) => (
                asset.exif_image_width.unwrap_or(0) as u32,
                asset.exif_image_height.unwrap_or(0) as u32,
            ),
        };

        let thumbhash = compute_thumbhash(&thumbnail_path).ok();

        Ok(Some(GeneratedOutputs {
            thumbhash,
            width: Some(width as i32),
            height: Some(height as i32),
        }))
    }

    async fn generate_image(
        &self,
        asset: &ThumbnailAssetJob,
        config: &ImageFormatConfig,
        is_edited: bool,
    ) -> Result<ThumbnailJobOutcome, String> {
        let preview_path = self.storage.image_derivative_path(
            &asset.owner_id,
            &asset.id,
            "preview",
            &config.preview_format,
            is_edited,
        );
        let thumbnail_path = self.storage.image_derivative_path(
            &asset.owner_id,
            &asset.id,
            "thumbnail",
            &config.thumbnail_format,
            is_edited,
        );
        let fullsize_path = if should_generate_fullsize(asset, config, is_edited) {
            Some(self.storage.image_derivative_path(
                &asset.owner_id,
                &asset.id,
                "fullsize",
                &config.fullsize_format,
                is_edited,
            ))
        } else {
            None
        };

        if !Path::new(&asset.original_path).exists() {
            return Ok(ThumbnailJobOutcome::Failed);
        }

        let decoded = match self.decode_asset_image(asset, config, is_edited).await {
            Ok((image, _, _)) => image,
            Err(err) => {
                eprintln!("image decode failed for {}, trying ffmpeg: {err}", asset.id);
                extract_with_ffmpeg(&asset.original_path, config.preview_size).await?
            }
        };

        write_resized(
            &decoded,
            &preview_path,
            config.preview_size,
            &config.preview_format,
            config.preview_quality,
        )?;
        write_resized(
            &decoded,
            &thumbnail_path,
            config.thumbnail_size,
            &config.thumbnail_format,
            config.thumbnail_quality,
        )?;
        if let Some(fullsize) = fullsize_path.as_ref() {
            write_resized(
                &decoded,
                fullsize,
                u32::MAX,
                &config.fullsize_format,
                config.fullsize_quality,
            )?;
        }

        let thumbhash = if is_edited {
            None
        } else {
            Some(compute_thumbhash(&thumbnail_path)?)
        };

        let mut upserts = vec![
            UpsertAssetFile {
                asset_id: asset.id,
                path: preview_path.to_string_lossy().into_owned(),
                file_type: "preview".into(),
                is_edited,
                is_progressive: false,
                is_transparent: false,
            },
            UpsertAssetFile {
                asset_id: asset.id,
                path: thumbnail_path.to_string_lossy().into_owned(),
                file_type: "thumbnail".into(),
                is_edited,
                is_progressive: false,
                is_transparent: false,
            },
        ];
        if let Some(fullsize) = fullsize_path.as_ref() {
            upserts.push(UpsertAssetFile {
                asset_id: asset.id,
                path: fullsize.to_string_lossy().into_owned(),
                file_type: "fullsize".into(),
                is_edited,
                is_progressive: false,
                is_transparent: false,
            });
        }

        self.sync_derivative_files_with_upserts(asset, &upserts, is_edited)
            .await?;

        if let Some(hash) = thumbhash.as_ref() {
            if asset
                .thumbhash
                .as_ref()
                .map(|existing| existing.as_slice() != hash.as_slice())
                .unwrap_or(true)
            {
                asset_job::update_thumbhash(&self.pool, &asset.id, hash)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }

        Ok(ThumbnailJobOutcome::Success)
    }

    async fn generate_video_like(
        &self,
        asset: &ThumbnailAssetJob,
        config: &ImageFormatConfig,
        is_edited: bool,
    ) -> Result<ThumbnailJobOutcome, String> {
        if is_edited {
            return Ok(ThumbnailJobOutcome::Skipped);
        }

        let preview_path = self.storage.image_derivative_path(
            &asset.owner_id,
            &asset.id,
            "preview",
            &config.preview_format,
            false,
        );
        let thumbnail_path = self.storage.image_derivative_path(
            &asset.owner_id,
            &asset.id,
            "thumbnail",
            &config.thumbnail_format,
            false,
        );

        if !Path::new(&asset.original_path).exists() {
            return Ok(ThumbnailJobOutcome::Failed);
        }

        let tonemap = self.load_ffmpeg_tonemap().await?;
        let video_ctx = if asset.asset_type == "VIDEO" {
            let (Some(video_index), Some(codec_name)) =
                (asset.video_index, asset.video_codec_name.clone())
            else {
                return Ok(ThumbnailJobOutcome::Failed);
            };
            Some(VideoThumbnailContext {
                tonemap,
                stream: VideoThumbnailStream {
                    video_index,
                    codec_name,
                    pixel_format: asset.pixel_format.clone(),
                    color_primaries: asset.color_primaries.unwrap_or(2),
                    color_transfer: asset.color_transfer.unwrap_or(2),
                    color_matrix: asset.color_matrix.unwrap_or(2),
                    format_name: asset.format_name.clone(),
                },
            })
        } else {
            None
        };

        render_with_ffmpeg(
            &asset.original_path,
            &preview_path,
            config.preview_size,
            &config.preview_format,
            config.preview_quality,
            video_ctx.as_ref(),
        )
        .await?;
        render_with_ffmpeg(
            &asset.original_path,
            &thumbnail_path,
            config.thumbnail_size,
            &config.thumbnail_format,
            config.thumbnail_quality,
            video_ctx.as_ref(),
        )
        .await?;

        let thumbhash = compute_thumbhash(&thumbnail_path)?;
        let upserts = vec![
            UpsertAssetFile {
                asset_id: asset.id,
                path: preview_path.to_string_lossy().into_owned(),
                file_type: "preview".into(),
                is_edited: false,
                is_progressive: false,
                is_transparent: false,
            },
            UpsertAssetFile {
                asset_id: asset.id,
                path: thumbnail_path.to_string_lossy().into_owned(),
                file_type: "thumbnail".into(),
                is_edited: false,
                is_progressive: false,
                is_transparent: false,
            },
        ];
        self.sync_derivative_files_with_upserts(asset, &upserts, false)
            .await?;

        if asset
            .thumbhash
            .as_ref()
            .map(|existing| existing.as_slice() != thumbhash.as_slice())
            .unwrap_or(true)
        {
            asset_job::update_thumbhash(&self.pool, &asset.id, &thumbhash)
                .await
                .map_err(|err| err.to_string())?;
        }

        Ok(ThumbnailJobOutcome::Success)
    }

    async fn decode_asset_image(
        &self,
        asset: &ThumbnailAssetJob,
        config: &ImageFormatConfig,
        is_edited: bool,
    ) -> Result<(DynamicImage, u32, u32), String> {
        let mut skip_orientation = false;
        let mut image = if config.extract_embedded && is_raw_file(&asset.original_file_name) {
            if let Some(preview) =
                extract_raw_embedded_preview(&asset.original_path, config.preview_size).await?
            {
                skip_orientation = true;
                decode_image_bytes(&preview).await?
            } else {
                extract_with_ffmpeg(&asset.original_path, config.preview_size).await?
            }
        } else {
            decode_image_path(&asset.original_path).await?
        };

        if !is_edited && !skip_orientation {
            image = apply_exif_orientation(image, asset.orientation.as_deref());
        }

        if is_edited {
            image = apply_edits(image, &asset.edits);
        }

        let (width, height) = image.dimensions();
        Ok((image, width, height))
    }

    async fn sync_derivative_files_with_upserts(
        &self,
        asset: &ThumbnailAssetJob,
        upserts: &[UpsertAssetFile],
        is_edited: bool,
    ) -> Result<(), String> {
        let new_paths: Vec<String> = upserts.iter().map(|file| file.path.clone()).collect();
        let mut paths_to_delete = Vec::new();
        for file in &asset.files {
            if file.is_edited != is_edited {
                continue;
            }
            if !new_paths.iter().any(|path| path == &file.path) {
                paths_to_delete.push(file.path.clone());
            }
        }

        asset_job::upsert_asset_files(&self.pool, upserts)
            .await
            .map_err(|err| err.to_string())?;

        if !paths_to_delete.is_empty() {
            let _ = self
                .jobs
                .queue_file_delete(&paths_to_delete)
                .await
                .map_err(|err| err.to_string());
        }

        Ok(())
    }
}

fn should_generate_fullsize(
    asset: &ThumbnailAssetJob,
    config: &ImageFormatConfig,
    is_edited: bool,
) -> bool {
    if asset.asset_type != "IMAGE" {
        return false;
    }
    is_edited
        || config.fullsize_enabled
        || asset.projection_type.as_deref() == Some("EQUIRECTANGULAR")
        || is_web_unsupported_file(&asset.original_file_name)
}

fn is_raw_file(filename: &str) -> bool {
    has_extension(filename, RAW_EXTENSIONS)
}

fn is_web_unsupported_file(filename: &str) -> bool {
    has_extension(filename, WEB_UNSUPPORTED_EXTENSIONS)
}

fn has_extension(filename: &str, extensions: &[&str]) -> bool {
    let lower = filename.to_ascii_lowercase();
    extensions.iter().any(|ext| lower.ends_with(ext))
}

fn crop_box_from_edits(edits: &[AssetEditRow]) -> Option<BoundingBox> {
    let crop = edits.iter().find(|edit| edit.action == "crop")?;
    let params = parse_crop(&crop.parameters)?;
    Some(BoundingBox {
        x1: params.x as f32,
        y1: params.y as f32,
        x2: (params.x + params.width as i32) as f32,
        y2: (params.y + params.height as i32) as f32,
    })
}

fn read_string(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

fn read_u32(value: &Value, key: &str, default: u32) -> u32 {
    value
        .get(key)
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(default)
}

async fn decode_image_path(path: &str) -> Result<DynamicImage, String> {
    tokio::task::spawn_blocking({
        let path = path.to_string();
        move || image::open(path).map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

async fn decode_image_bytes(data: &[u8]) -> Result<DynamicImage, String> {
    let data = data.to_vec();
    tokio::task::spawn_blocking(move || {
        image::load_from_memory(&data).map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

const RAW_EMBEDDED_PREVIEW_TAGS: &[&str] =
    &["JpgFromRaw2", "JpgFromRaw", "PreviewJXL", "PreviewImage"];

async fn extract_raw_embedded_preview(
    path: &str,
    min_size: u32,
) -> Result<Option<Vec<u8>>, String> {
    for tag in RAW_EMBEDDED_PREVIEW_TAGS {
        let Ok(data) = exiftool::extract_binary_tag(path, tag).await else {
            continue;
        };
        if data.is_empty() {
            continue;
        }
        if should_use_embedded_preview(&data, min_size) {
            return Ok(Some(data));
        }
    }
    Ok(None)
}

fn should_use_embedded_preview(data: &[u8], target_size: u32) -> bool {
    image::load_from_memory(data)
        .ok()
        .map(|image| {
            let (width, height) = image.dimensions();
            width.min(height) >= target_size
        })
        .unwrap_or(false)
}

async fn extract_with_ffmpeg(input: &str, size: u32) -> Result<DynamicImage, String> {
    let temp = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()
        .map_err(|err| err.to_string())?;
    let temp_path = temp.path().to_path_buf();
    render_with_ffmpeg(input, &temp_path, size, "png", 90, None).await?;
    decode_image_path(temp_path.to_str().unwrap()).await
}

#[derive(Debug, Clone)]
struct VideoThumbnailContext {
    tonemap: String,
    stream: VideoThumbnailStream,
}

async fn render_with_ffmpeg(
    input: &str,
    output: &Path,
    size: u32,
    format: &str,
    quality: u8,
    video: Option<&VideoThumbnailContext>,
) -> Result<(), String> {
    StoragePaths::ensure_parent(output).map_err(|err| err.to_string())?;

    let vf = if let Some(video) = video {
        build_video_thumbnail_vf(&video.tonemap, &video.stream, size)
    } else {
        format!("scale={size}:{size}:force_original_aspect_ratio=decrease")
    };

    let mut args = vec![
        "ffmpeg".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
    ];

    if let Some(video) = video {
        append_video_thumbnail_input_args(&mut args, &video.stream);
    }

    args.extend([
        "-autorotate".into(),
        "1".into(),
        "-i".into(),
        input.into(),
        "-vf".into(),
        vf,
        "-frames:v".into(),
        "1".into(),
        "-fps_mode".into(),
        "vfr".into(),
    ]);

    match format {
        "webp" => {
            args.extend([
                "-c:v".into(),
                "libwebp".into(),
                "-quality".into(),
                quality.to_string(),
            ]);
        }
        "jpeg" | "jpg" => {
            args.extend(["-q:v".into(), map_jpeg_quality(quality).to_string()]);
        }
        "png" => {}
        _ => {
            args.extend(["-q:v".into(), map_jpeg_quality(quality).to_string()]);
        }
    }

    args.extend([
        "-update".into(),
        "1".into(),
        output.to_string_lossy().into_owned(),
    ]);

    let output_result = Command::new(&args[0])
        .args(&args[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|err| format!("failed to run ffmpeg: {err}"))?;

    if !output_result.status.success() {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        return Err(format!("ffmpeg failed: {stderr}"));
    }

    Ok(())
}

fn map_jpeg_quality(quality: u8) -> u8 {
    let q = 31.0 - (quality as f32 / 100.0) * 29.0;
    q.round().clamp(2.0, 31.0) as u8
}

fn write_resized(
    image: &DynamicImage,
    output: &Path,
    size: u32,
    format: &str,
    _quality: u8,
) -> Result<(), String> {
    StoragePaths::ensure_parent(output).map_err(|err| err.to_string())?;
    let (width, height) = image.dimensions();
    let longest = width.max(height).max(1);
    let resized = if size == u32::MAX || longest <= size {
        image.clone()
    } else {
        let (new_w, new_h) = if width >= height {
            (
                size,
                ((height as f64 * size as f64) / width as f64).round() as u32,
            )
        } else {
            (
                ((width as f64 * size as f64) / height as f64).round() as u32,
                size,
            )
        };
        image.resize(new_w.max(1), new_h.max(1), FilterType::Lanczos3)
    };

    let image_format = match format {
        "webp" => ImageFormat::WebP,
        "png" => ImageFormat::Png,
        _ => ImageFormat::Jpeg,
    };

    resized
        .save_with_format(output, image_format)
        .map_err(|err| err.to_string())
}

fn compute_thumbhash(path: &Path) -> Result<Vec<u8>, String> {
    let rgba = image::open(path).map_err(|err| err.to_string())?;
    compute_thumbhash_from_image(&rgba)
}

fn compute_thumbhash_from_image(rgba: &DynamicImage) -> Result<Vec<u8>, String> {
    let rgba = rgba.to_rgba8();
    let (width, height) = rgba.dimensions();
    let (width, height) = fit_thumbhash_dimensions(width, height);
    let resized = image::imageops::resize(&rgba, width, height, FilterType::Triangle);
    Ok(rgba_to_thumb_hash(
        width as usize,
        height as usize,
        resized.as_raw(),
    ))
}

fn fit_thumbhash_dimensions(width: u32, height: u32) -> (u32, u32) {
    let max = 100;
    if width <= max && height <= max {
        return (width.max(1), height.max(1));
    }
    if width >= height {
        let new_h = ((height as f64 * max as f64) / width as f64)
            .round()
            .max(1.0) as u32;
        (max, new_h)
    } else {
        let new_w = ((width as f64 * max as f64) / height as f64)
            .round()
            .max(1.0) as u32;
        (new_w, max)
    }
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgb};

    use super::should_use_embedded_preview;

    #[test]
    fn accepts_embedded_preview_when_long_edge_meets_target_size() {
        let image = ImageBuffer::from_fn(1440, 1080, |_, _| Rgb([0u8, 0, 0]));
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Jpeg,
            )
            .expect("jpeg encode");

        assert!(should_use_embedded_preview(&bytes, 1080));
        assert!(!should_use_embedded_preview(&bytes, 2000));
    }
}
