use std::path::Path;
use std::process::Stdio;

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageFormat};
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
use crate::service::job::{EntityJob, JobService, PersonJob};
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

const WEB_SUPPORTED_EXTENSIONS: &[&str] =
    &[".avif", ".bmp", ".gif", ".jpeg", ".jpg", ".png", ".webp"];

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
    preview_progressive: bool,
    thumbnail_format: String,
    thumbnail_size: u32,
    thumbnail_quality: u8,
    thumbnail_progressive: bool,
    fullsize_enabled: bool,
    fullsize_format: String,
    fullsize_quality: u8,
    fullsize_progressive: bool,
    colorspace: String,
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
            preview_progressive: false,
            thumbnail_quality: 80,
            thumbnail_progressive: false,
            fullsize_enabled: false,
            fullsize_format: "jpeg".into(),
            fullsize_quality: 80,
            fullsize_progressive: false,
            colorspace: "p3".into(),
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
            tracing::error!(
                "thumbnail generation failed for {asset_id}: missing asset or metadata"
            );
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
            tracing::error!(
                "skipping thumbnail generation for {}: type {} is not image/video",
                asset.id,
                asset.asset_type
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
            tracing::error!(
                "edit thumbnail generation failed for {asset_id}: missing asset or metadata"
            );
            return Ok(ThumbnailJobOutcome::Failed);
        };

        let config = self.load_image_config().await?;
        let generated = self
            .generate_edited_image_derivatives(&asset, &config)
            .await?;

        let mut thumbhash = generated.as_ref().and_then(|g| g.thumbhash.clone());

        if thumbhash.is_none() {
            if let Ok(decoded) = self.decode_asset_image(&asset, &config, false).await {
                thumbhash = Some(compute_thumbhash_from_image(&decoded.image)?);
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
        owner_id: &Uuid,
        person_id: &Uuid,
    ) -> Result<ThumbnailJobOutcome, String> {
        let Some(data) = asset_job::get_person_thumbnail_job_data(&self.pool, owner_id, person_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            tracing::error!("person thumbnail generation failed for {person_id}: missing data");
            return Ok(ThumbnailJobOutcome::Failed);
        };

        let input_path = if data.asset_type == "VIDEO" {
            let Some(preview) = data.preview_path.as_ref() else {
                tracing::error!(
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
        let (decoded, kind) = if data.asset_type == "VIDEO" {
            match decode_image_path(&input_path).await {
                Ok(image) => (image, StillDecodeKind::VideoPreview),
                Err(err) => {
                    tracing::error!("person thumbnail decode failed for {person_id}: {err}");
                    (
                        extract_with_ffmpeg(&input_path, config.preview_size).await?,
                        StillDecodeKind::VideoPreview,
                    )
                }
            }
        } else if config.extract_embedded && is_raw_file(&data.original_path) {
            if let Some(preview) =
                extract_raw_embedded_preview(&data.original_path, config.preview_size).await?
            {
                (
                    decode_image_bytes(&preview.bytes).await?,
                    StillDecodeKind::EmbeddedPreview,
                )
            } else {
                (
                    extract_with_ffmpeg(&data.original_path, config.preview_size).await?,
                    StillDecodeKind::FfmpegAutorotated,
                )
            }
        } else {
            match decode_image_path(&input_path).await {
                Ok(image) => (image, StillDecodeKind::ImageFile),
                Err(err) => {
                    tracing::error!("person thumbnail decode failed for {person_id}: {err}");
                    (
                        extract_with_ffmpeg(&input_path, config.preview_size).await?,
                        StillDecodeKind::FfmpegAutorotated,
                    )
                }
            }
        };

        let oriented = if should_apply_person_orientation(&data.asset_type, kind) {
            apply_exif_orientation(decoded, data.exif_orientation.as_deref())
        } else {
            decoded
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
            false,
        )?;

        asset_job::update_person_thumbnail_path(
            &self.pool,
            owner_id,
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
        let mut person_batch: Vec<PersonJob> = Vec::new();
        for person in people {
            if person.face_asset_id.is_none() {
                if let Some(face_id) = asset_job::get_random_face_id(&self.pool, &person.id)
                    .await
                    .map_err(|err| err.to_string())?
                {
                    asset_job::update_person_face_asset_id(
                        &self.pool,
                        &person.owner_id,
                        &person.id,
                        &face_id,
                    )
                    .await
                    .map_err(|err| err.to_string())?;
                } else {
                    continue;
                }
            }

            person_batch.push(PersonJob {
                owner_id: person.owner_id,
                person_group_id: person.id,
            });
            if person_batch.len() >= JOBS_BATCH_SIZE {
                self.flush_person_queue_batch(&person_batch).await?;
                person_batch.clear();
            }
        }
        self.flush_person_queue_batch(&person_batch).await?;

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

    async fn flush_person_queue_batch(&self, batch: &[PersonJob]) -> Result<(), String> {
        for job in batch {
            self.jobs
                .queue_person_generate_thumbnail(&job.owner_id, &job.person_group_id)
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
                config.preview_progressive =
                    read_bool(preview, "progressive", config.preview_progressive);
            }
            if let Some(thumbnail) = image.get("thumbnail") {
                config.thumbnail_format =
                    read_string(thumbnail, "format", &config.thumbnail_format);
                config.thumbnail_size = read_u32(thumbnail, "size", config.thumbnail_size);
                config.thumbnail_quality =
                    read_u32(thumbnail, "quality", config.thumbnail_quality as u32) as u8;
                config.thumbnail_progressive =
                    read_bool(thumbnail, "progressive", config.thumbnail_progressive);
            }
            if let Some(fullsize) = image.get("fullsize") {
                config.fullsize_enabled = fullsize
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                config.fullsize_format = read_string(fullsize, "format", &config.fullsize_format);
                config.fullsize_quality =
                    read_u32(fullsize, "quality", config.fullsize_quality as u32) as u8;
                config.fullsize_progressive =
                    read_bool(fullsize, "progressive", config.fullsize_progressive);
            }
            config.colorspace = read_string(&image, "colorspace", &config.colorspace);
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
            Ok(decoded) => output_dimensions(
                &asset.edits,
                decoded.pre_edit_width,
                decoded.pre_edit_height,
            ),
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
        if !Path::new(&asset.original_path).exists() {
            return Ok(ThumbnailJobOutcome::Failed);
        }

        let decoded = match self.decode_asset_image(asset, config, is_edited).await {
            Ok(decoded) => decoded,
            Err(err) => {
                tracing::error!("image decode failed for {}, trying ffmpeg: {err}", asset.id);
                let image = extract_with_ffmpeg(&asset.original_path, config.preview_size).await?;
                let (width, height) = image.dimensions();
                DecodedStill {
                    image,
                    embedded_jpeg: None,
                    pre_edit_width: width,
                    pre_edit_height: height,
                }
            }
        };

        let generate_fullsize = should_generate_fullsize(asset, config, is_edited);
        let copy_embedded = generate_fullsize && decoded.embedded_jpeg.is_some();
        let fullsize_path = if generate_fullsize {
            let (format, edited) =
                fullsize_derivative(copy_embedded, &config.fullsize_format, is_edited);
            Some(self.storage.image_derivative_path(
                &asset.owner_id,
                &asset.id,
                "fullsize",
                format,
                edited,
            ))
        } else {
            None
        };

        write_resized(
            &decoded.image,
            &preview_path,
            config.preview_size,
            &config.preview_format,
            config.preview_quality,
            config.preview_progressive,
        )?;
        write_resized(
            &decoded.image,
            &thumbnail_path,
            config.thumbnail_size,
            &config.thumbnail_format,
            config.thumbnail_quality,
            config.thumbnail_progressive,
        )?;
        if let Some(fullsize) = fullsize_path.as_ref() {
            if let Some(bytes) = decoded.embedded_jpeg.as_deref() {
                write_bytes(fullsize, bytes).await?;
                if let Err(err) = exiftool::write_orientation_and_colorspace(
                    &fullsize.to_string_lossy(),
                    asset.orientation.as_deref(),
                    asset.colorspace.as_deref(),
                )
                .await
                {
                    tracing::warn!("could not write fullsize exif to {fullsize:?}: {err}");
                }
            } else {
                write_resized(
                    &decoded.image,
                    fullsize,
                    u32::MAX,
                    &config.fullsize_format,
                    config.fullsize_quality,
                    config.fullsize_progressive,
                )?;
            }
        }

        let output_space = output_colorspace(
            asset.colorspace.as_deref(),
            asset.profile_description.as_deref(),
            asset.bits_per_sample,
            &config.colorspace,
        );
        let source_is_srgb = is_srgb(
            asset.colorspace.as_deref(),
            asset.profile_description.as_deref(),
            asset.bits_per_sample,
        );
        if should_preserve_source_icc(source_is_srgb, output_space) {
            let icc_fullsize = if copy_embedded {
                None
            } else {
                fullsize_path.as_deref()
            };
            copy_source_icc_profile(
                &asset.original_path,
                &preview_path,
                &thumbnail_path,
                icc_fullsize,
            )
            .await;
        } else {
            tracing::debug!(
                "asset {} is not sRGB and image.colorspace is srgb; thumbnail pixels are left unconverted",
                asset.id
            );
        }

        if asset.projection_type.as_deref() == Some("EQUIRECTANGULAR") {
            copy_equirectangular_pano_tags(
                &asset.original_path,
                &preview_path,
                fullsize_path.as_deref(),
            )
            .await;
        }

        let is_transparent = image_file_is_transparent(asset, config);
        warn_transparency_loss(asset.id, is_transparent, &config.preview_format);
        warn_transparency_loss(asset.id, is_transparent, &config.thumbnail_format);
        if generate_fullsize && !copy_embedded {
            warn_transparency_loss(asset.id, is_transparent, &config.fullsize_format);
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
                is_progressive: is_progressive_output(
                    &config.preview_format,
                    config.preview_progressive,
                ),
                is_transparent,
            },
            UpsertAssetFile {
                asset_id: asset.id,
                path: thumbnail_path.to_string_lossy().into_owned(),
                file_type: "thumbnail".into(),
                is_edited,
                is_progressive: is_progressive_output(
                    &config.thumbnail_format,
                    config.thumbnail_progressive,
                ),
                is_transparent,
            },
        ];
        if let Some(fullsize) = fullsize_path.as_ref() {
            upserts.push(UpsertAssetFile {
                asset_id: asset.id,
                path: fullsize.to_string_lossy().into_owned(),
                file_type: "fullsize".into(),
                is_edited: if copy_embedded { false } else { is_edited },
                is_progressive: is_progressive_output(
                    &config.fullsize_format,
                    config.fullsize_progressive,
                ),
                is_transparent,
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
    ) -> Result<DecodedStill, String> {
        let mut embedded_jpeg = None;
        let (mut image, kind) = if config.extract_embedded && is_raw_file(&asset.original_file_name)
        {
            if let Some(preview) =
                extract_raw_embedded_preview(&asset.original_path, config.preview_size).await?
            {
                let image = decode_image_bytes(&preview.bytes).await?;
                if preview.kind == EmbeddedPreviewKind::Jpeg {
                    embedded_jpeg = Some(preview.bytes);
                }
                (image, StillDecodeKind::EmbeddedPreview)
            } else {
                (
                    extract_with_ffmpeg(&asset.original_path, config.preview_size).await?,
                    StillDecodeKind::FfmpegAutorotated,
                )
            }
        } else {
            (
                decode_image_path(&asset.original_path).await?,
                StillDecodeKind::ImageFile,
            )
        };

        if should_apply_stored_orientation(kind) {
            image = apply_exif_orientation(image, asset.orientation.as_deref());
        }

        let (pre_edit_width, pre_edit_height) = image.dimensions();
        if is_edited {
            image = apply_edits(image, &asset.edits);
        }

        Ok(DecodedStill {
            image,
            embedded_jpeg,
            pre_edit_width,
            pre_edit_height,
        })
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

/// Re-attach 360° panorama metadata after JPEG/WebP re-encode strips it.
/// Failures are warnings: thumbnail generation still succeeds, matching TypeScript.
async fn copy_equirectangular_pano_tags(source: &str, preview: &Path, fullsize: Option<&Path>) {
    if let Err(err) =
        exiftool::copy_tag_group("XMP-GPano", source, &preview.to_string_lossy()).await
    {
        tracing::warn!("could not copy XMP-GPano tags to preview {preview:?}: {err}");
    }
    if let Some(fullsize) = fullsize {
        if let Err(err) =
            exiftool::copy_tag_group("XMP-GPano", source, &fullsize.to_string_lossy()).await
        {
            tracing::warn!("could not copy XMP-GPano tags to fullsize {fullsize:?}: {err}");
        }
    }
}

/// TypeScript `MediaService.isSRGB`. Empty strings count as missing metadata.
fn is_srgb(
    colorspace: Option<&str>,
    profile_description: Option<&str>,
    bits_per_sample: Option<i32>,
) -> bool {
    let colorspace = nonempty(colorspace);
    let profile_description = nonempty(profile_description);
    if colorspace.is_some() || profile_description.is_some() {
        return [colorspace, profile_description]
            .into_iter()
            .flatten()
            .any(|value| value.to_ascii_lowercase().contains("srgb"));
    }
    match bits_per_sample {
        Some(bits) => bits == 8,
        None => true,
    }
}

fn output_colorspace(
    colorspace: Option<&str>,
    profile_description: Option<&str>,
    bits_per_sample: Option<i32>,
    configured: &str,
) -> &'static str {
    if is_srgb(colorspace, profile_description, bits_per_sample) {
        "srgb"
    } else if configured.eq_ignore_ascii_case("srgb") {
        "srgb"
    } else {
        "p3"
    }
}

/// Pixels are not converted. Keep the source ICC profile when that profile
/// matches the colorspace TypeScript would embed after conversion.
fn should_preserve_source_icc(source_is_srgb: bool, output_colorspace: &str) -> bool {
    source_is_srgb || !output_colorspace.eq_ignore_ascii_case("srgb")
}

/// WebP derivatives are never marked progressive. Matches `format !== ImageFormat.Webp`.
fn is_progressive_output(format: &str, progressive: bool) -> bool {
    progressive && !format.eq_ignore_ascii_case("webp")
}

fn can_be_transparent(filename: &str) -> bool {
    const EXTENSIONS: &[&str] = &[
        ".avif", ".bmp", ".gif", ".heic", ".heif", ".hif", ".jxl", ".png", ".svg", ".tif", ".tiff",
        ".webp",
    ];
    has_extension(filename, EXTENSIONS)
}

fn image_file_is_transparent(asset: &ThumbnailAssetJob, config: &ImageFormatConfig) -> bool {
    if config.extract_embedded && is_raw_file(&asset.original_file_name) {
        return false;
    }
    can_be_transparent(&asset.original_file_name) && file_has_alpha(&asset.original_path)
}

fn file_has_alpha(path: &str) -> bool {
    let Ok(reader) = image::ImageReader::open(path) else {
        return false;
    };
    let Ok(reader) = reader.with_guessed_format() else {
        return false;
    };
    reader
        .into_decoder()
        .map(|decoder| decoder.color_type().has_alpha())
        .unwrap_or(false)
}

fn warn_transparency_loss(asset_id: Uuid, is_transparent: bool, format: &str) {
    if is_transparent && is_jpeg(format) {
        tracing::warn!(
            "Asset {asset_id} has transparency but the configured format is jpeg which does not support it, consider using a format that does, such as webp"
        );
    }
}

fn is_jpeg(format: &str) -> bool {
    matches!(format.to_ascii_lowercase().as_str(), "jpeg" | "jpg")
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.filter(|text| !text.is_empty())
}

async fn copy_source_icc_profile(
    source: &str,
    preview: &Path,
    thumbnail: &Path,
    fullsize: Option<&Path>,
) {
    for target in [preview, thumbnail].into_iter().chain(fullsize) {
        if let Err(err) = exiftool::copy_icc_profile(source, &target.to_string_lossy()).await {
            tracing::warn!("could not copy ICC profile to {target:?}: {err}");
        }
    }
}

fn should_generate_fullsize(
    asset: &ThumbnailAssetJob,
    config: &ImageFormatConfig,
    is_edited: bool,
) -> bool {
    needs_fullsize_derivative(
        asset.asset_type == "IMAGE",
        is_edited,
        config.fullsize_enabled,
        asset.projection_type.as_deref(),
        &asset.original_file_name,
    )
}

/// TypeScript `isGenerateFullsize`: edits, or (fullsize/360°) on a file the web client cannot show.
fn needs_fullsize_derivative(
    is_image: bool,
    is_edited: bool,
    fullsize_enabled: bool,
    projection_type: Option<&str>,
    filename: &str,
) -> bool {
    if !is_image {
        return false;
    }
    if is_edited {
        return true;
    }
    (fullsize_enabled || projection_type == Some("EQUIRECTANGULAR"))
        && !is_web_supported_file(filename)
}

/// Camera JPEG embedded in a RAW is stored as the fullsize file. Other fullsize outputs are re-encoded.
fn fullsize_derivative<'a>(
    copy_embedded_jpeg: bool,
    configured_format: &'a str,
    is_edited: bool,
) -> (&'a str, bool) {
    if copy_embedded_jpeg {
        ("jpeg", false)
    } else {
        (configured_format, is_edited)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StillDecodeKind {
    ImageFile,
    EmbeddedPreview,
    FfmpegAutorotated,
    VideoPreview,
}

/// `image` does not apply EXIF orientation. ffmpeg `-autorotate 1` and video preview JPEGs already did.
fn should_apply_stored_orientation(kind: StillDecodeKind) -> bool {
    matches!(
        kind,
        StillDecodeKind::ImageFile | StillDecodeKind::EmbeddedPreview
    )
}

fn should_apply_person_orientation(asset_type: &str, kind: StillDecodeKind) -> bool {
    asset_type != "VIDEO" && should_apply_stored_orientation(kind)
}

struct DecodedStill {
    image: DynamicImage,
    embedded_jpeg: Option<Vec<u8>>,
    pre_edit_width: u32,
    pre_edit_height: u32,
}

fn is_raw_file(filename: &str) -> bool {
    has_extension(filename, RAW_EXTENSIONS)
}

fn is_web_supported_file(filename: &str) -> bool {
    has_extension(filename, WEB_SUPPORTED_EXTENSIONS)
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

fn read_bool(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmbeddedPreviewKind {
    Jpeg,
    Jxl,
}

struct EmbeddedPreview {
    bytes: Vec<u8>,
    kind: EmbeddedPreviewKind,
}

const RAW_EMBEDDED_PREVIEW_TAGS: &[(&str, EmbeddedPreviewKind)] = &[
    ("JpgFromRaw2", EmbeddedPreviewKind::Jpeg),
    ("JpgFromRaw", EmbeddedPreviewKind::Jpeg),
    ("PreviewJXL", EmbeddedPreviewKind::Jxl),
    ("PreviewImage", EmbeddedPreviewKind::Jpeg),
];

async fn extract_raw_embedded_preview(
    path: &str,
    min_size: u32,
) -> Result<Option<EmbeddedPreview>, String> {
    for (tag, kind) in RAW_EMBEDDED_PREVIEW_TAGS {
        let Ok(data) = exiftool::extract_binary_tag(path, tag).await else {
            continue;
        };
        if data.is_empty() {
            continue;
        }
        if should_use_embedded_preview(&data, min_size) {
            return Ok(Some(EmbeddedPreview {
                bytes: data,
                kind: *kind,
            }));
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

fn jpeg_sampling_factor(quality: u8) -> jpeg_encoder::SamplingFactor {
    if quality >= 80 {
        jpeg_encoder::SamplingFactor::R_4_4_4
    } else {
        jpeg_encoder::SamplingFactor::R_4_2_0
    }
}

fn encode_jpeg(
    image: &DynamicImage,
    output: &Path,
    quality: u8,
    progressive: bool,
) -> Result<(), String> {
    let rgb = image.to_rgb8();
    let (width, height) = rgb.dimensions();
    let (Ok(width), Ok(height)) = (u16::try_from(width), u16::try_from(height)) else {
        tracing::warn!(
            "image {width}x{height} exceeds the progressive JPEG encoder limit; writing a baseline JPEG"
        );
        return encode_jpeg_baseline(image, output, quality);
    };

    let bytes = encode_jpeg_bytes(rgb.as_raw(), width, height, quality, progressive)?;
    std::fs::write(output, bytes).map_err(|err| err.to_string())
}

fn encode_jpeg_bytes(
    rgb: &[u8],
    width: u16,
    height: u16,
    quality: u8,
    progressive: bool,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut bytes, quality);
    encoder.set_progressive(progressive);
    encoder.set_sampling_factor(jpeg_sampling_factor(quality));
    encoder
        .encode(rgb, width, height, jpeg_encoder::ColorType::Rgb)
        .map_err(|err| err.to_string())?;
    Ok(bytes)
}

fn encode_jpeg_baseline(image: &DynamicImage, output: &Path, quality: u8) -> Result<(), String> {
    use image::codecs::jpeg::JpegEncoder;
    use std::fs::File;
    use std::io::BufWriter;
    let file = File::create(output).map_err(|err| err.to_string())?;
    let encoder = JpegEncoder::new_with_quality(BufWriter::new(file), quality);
    image
        .write_with_encoder(encoder)
        .map_err(|err| err.to_string())
}

fn jpeg_is_progressive(bytes: &[u8]) -> bool {
    bytes.windows(2).any(|marker| marker == [0xFF, 0xC2])
}

async fn write_bytes(output: &Path, bytes: &[u8]) -> Result<(), String> {
    StoragePaths::ensure_parent(output).map_err(|err| err.to_string())?;
    tokio::fs::write(output, bytes)
        .await
        .map_err(|err| err.to_string())
}

fn write_resized(
    image: &DynamicImage,
    output: &Path,
    size: u32,
    format: &str,
    quality: u8,
    progressive: bool,
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

    match format {
        "jpeg" | "jpg" => encode_jpeg(&resized, output, quality, progressive),
        "png" => resized
            .save_with_format(output, ImageFormat::Png)
            .map_err(|err| err.to_string()),
        _ => {
            let _ = quality;
            resized
                .save_with_format(output, ImageFormat::WebP)
                .map_err(|err| err.to_string())
        }
    }
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

    #[test]
    fn srgb_detection_matches_typescript() {
        assert!(super::is_srgb(None, None, None));
        assert!(super::is_srgb(None, None, Some(8)));
        assert!(!super::is_srgb(None, None, Some(16)));
        assert!(!super::is_srgb(Some("Display P3"), None, Some(8)));
        assert!(super::is_srgb(
            Some(""),
            Some("sRGB IEC61966-2.1"),
            Some(16)
        ));
        assert_eq!(
            super::output_colorspace(Some("Display P3"), None, Some(8), "p3"),
            "p3"
        );
        assert_eq!(
            super::output_colorspace(Some("Display P3"), None, Some(8), "srgb"),
            "srgb"
        );
        assert!(super::should_preserve_source_icc(false, "p3"));
        assert!(!super::should_preserve_source_icc(false, "srgb"));
    }

    #[test]
    fn progressive_and_transparency_flags_match_typescript() {
        assert!(super::is_progressive_output("jpeg", true));
        assert!(!super::is_progressive_output("webp", true));
        assert!(!super::is_progressive_output("jpeg", false));
        assert!(super::can_be_transparent("photo.PNG"));
        assert!(!super::can_be_transparent("photo.jpg"));
    }

    #[test]
    fn jpeg_encoder_writes_progressive_scans_and_chroma() {
        use image::{ImageBuffer, Rgb};
        let image = ImageBuffer::from_fn(8, 8, |x, y| Rgb([(x * 30) as u8, (y * 30) as u8, 40]));
        let progressive = super::encode_jpeg_bytes(image.as_raw(), 8, 8, 80, true).expect("encode");
        let baseline = super::encode_jpeg_bytes(image.as_raw(), 8, 8, 80, false).expect("encode");
        assert!(super::jpeg_is_progressive(&progressive));
        assert!(!super::jpeg_is_progressive(&baseline));
        assert_eq!(
            super::jpeg_sampling_factor(80),
            jpeg_encoder::SamplingFactor::R_4_4_4
        );
        assert_eq!(
            super::jpeg_sampling_factor(79),
            jpeg_encoder::SamplingFactor::R_4_2_0
        );
    }

    #[test]
    fn fullsize_and_orientation_match_typescript() {
        assert!(super::needs_fullsize_derivative(
            true,
            false,
            true,
            None,
            "photo.cr3"
        ));
        assert!(!super::needs_fullsize_derivative(
            true,
            false,
            true,
            None,
            "photo.jpg"
        ));
        assert!(super::needs_fullsize_derivative(
            true,
            false,
            false,
            Some("EQUIRECTANGULAR"),
            "photo.tif"
        ));
        assert!(!super::needs_fullsize_derivative(
            true,
            false,
            false,
            Some("EQUIRECTANGULAR"),
            "photo.jpg"
        ));
        assert!(super::needs_fullsize_derivative(
            true,
            true,
            false,
            None,
            "photo.jpg"
        ));
        assert!(!super::needs_fullsize_derivative(
            false, true, true, None, "clip.mp4"
        ));

        assert_eq!(
            super::fullsize_derivative(true, "webp", true),
            ("jpeg", false)
        );
        assert_eq!(
            super::fullsize_derivative(false, "webp", true),
            ("webp", true)
        );

        assert!(super::should_apply_stored_orientation(
            super::StillDecodeKind::EmbeddedPreview
        ));
        assert!(super::should_apply_stored_orientation(
            super::StillDecodeKind::ImageFile
        ));
        assert!(!super::should_apply_stored_orientation(
            super::StillDecodeKind::FfmpegAutorotated
        ));
        assert!(!super::should_apply_person_orientation(
            "VIDEO",
            super::StillDecodeKind::VideoPreview
        ));
        assert!(super::should_apply_person_orientation(
            "IMAGE",
            super::StillDecodeKind::EmbeddedPreview
        ));
    }
}
