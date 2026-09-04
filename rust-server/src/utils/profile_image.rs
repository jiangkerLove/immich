use std::path::PathBuf;

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::system_metadata::get_json;
use crate::utils::storage::StoragePaths;

#[derive(Debug, Clone)]
struct ThumbnailConfig {
    format: String,
    size: u32,
    quality: u8,
}

impl Default for ThumbnailConfig {
    fn default() -> Self {
        Self {
            format: "webp".into(),
            size: 250,
            quality: 80,
        }
    }
}

/// Generate a profile thumbnail from raw upload bytes (TS `generateProfileImage`).
pub async fn generate_profile_image(
    pool: &PgPool,
    storage: &StoragePaths,
    user_id: &Uuid,
    input: &[u8],
) -> Result<PathBuf, String> {
    let config = load_thumbnail_config(pool).await?;
    let decoded = image::load_from_memory(input).map_err(|err| err.to_string())?;

    let file_id = Uuid::new_v4().to_string();
    let extension = match config.format.as_str() {
        "jpeg" | "jpg" => "jpeg",
        "png" => "png",
        "webp" => "webp",
        other => other,
    };
    let output = storage.profile_image_path(user_id, &file_id, extension);
    let output_for_write = output.clone();
    let format = config.format.clone();
    let size = config.size;
    let quality = config.quality;
    tokio::task::spawn_blocking(move || {
        write_resized(&decoded, &output_for_write, size, &format, quality)
    })
    .await
    .map_err(|err| err.to_string())??;
    Ok(output)
}

async fn load_thumbnail_config(pool: &PgPool) -> Result<ThumbnailConfig, String> {
    let mut config = ThumbnailConfig::default();
    let stored = get_json(pool, "system-config")
        .await
        .map_err(|err| err.to_string())?;
    if let Some(thumbnail) = stored
        .as_ref()
        .and_then(|value| value.get("image"))
        .and_then(|image| image.get("thumbnail"))
    {
        if let Some(format) = thumbnail.get("format").and_then(|v| v.as_str()) {
            config.format = format.to_string();
        }
        if let Some(size) = thumbnail.get("size").and_then(|v| v.as_u64()) {
            config.size = size as u32;
        }
        if let Some(quality) = thumbnail.get("quality").and_then(|v| v.as_u64()) {
            config.quality = quality as u8;
        }
    }
    Ok(config)
}

fn write_resized(
    image: &DynamicImage,
    output: &PathBuf,
    size: u32,
    format: &str,
    _quality: u8,
) -> Result<(), String> {
    StoragePaths::ensure_parent(output).map_err(|err| err.to_string())?;
    let (width, height) = image.dimensions();
    let longest = width.max(height).max(1);
    let resized = if longest <= size {
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
