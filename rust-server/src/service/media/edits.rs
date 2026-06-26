use image::imageops::{crop_imm, flip_horizontal_in_place, flip_vertical_in_place, rotate90, rotate180, rotate270};
use image::{DynamicImage, GenericImageView};
use serde_json::Value;

use crate::models::db::asset_edit::AssetEditRow;

#[derive(Debug, Clone)]
pub struct CropParams {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub fn apply_edits(image: DynamicImage, edits: &[AssetEditRow]) -> DynamicImage {
    let mut current = image;
    let crop = edits.iter().find(|edit| edit.action == "crop");
    if let Some(edit) = crop {
        if let Some(params) = parse_crop(&edit.parameters) {
            current = crop_image(current, &params);
        }
    }

    for edit in edits.iter().filter(|edit| edit.action != "crop") {
        current = match edit.action.as_str() {
            "rotate" => {
                let angle = edit
                    .parameters
                    .get("angle")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                rotate_image(current, angle)
            }
            "mirror" => {
                let axis = edit
                    .parameters
                    .get("axis")
                    .and_then(|v| v.as_str())
                    .unwrap_or("horizontal");
                mirror_image(current, axis)
            }
            _ => current,
        };
    }

    current
}

pub fn parse_crop(parameters: &Value) -> Option<CropParams> {
    let x = parameters.get("x")?.as_i64()? as i32;
    let y = parameters.get("y")?.as_i64()? as i32;
    let width = parameters.get("width")?.as_u64()? as u32;
    let height = parameters.get("height")?.as_u64()? as u32;
    if width == 0 || height == 0 {
        return None;
    }
    Some(CropParams {
        x,
        y,
        width,
        height,
    })
}

pub fn output_dimensions(
    edits: &[AssetEditRow],
    width: u32,
    height: u32,
) -> (u32, u32) {
    let mut w = width;
    let mut h = height;

    if let Some(crop) = edits.iter().find(|edit| edit.action == "crop") {
        if let Some(params) = parse_crop(&crop.parameters) {
            w = params.width;
            h = params.height;
        }
    }

    for edit in edits {
        if edit.action == "rotate" {
            let angle = edit
                .parameters
                .get("angle")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if angle == 90 || angle == 270 {
                std::mem::swap(&mut w, &mut h);
            }
        }
    }

    (w.max(1), h.max(1))
}

pub fn face_crop_from_bbox(
    old_width: i32,
    old_height: i32,
    new_width: u32,
    new_height: u32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
) -> CropParams {
    let clamp = |value: f32, min: i32, max: i32| -> f32 { value.clamp(min as f32, max as f32) };

    let clamped_x1 = clamp(x1, 0, old_width);
    let clamped_y1 = clamp(y1, 0, old_height);
    let clamped_x2 = clamp(x2, 0, old_width);
    let clamped_y2 = clamp(y2, 0, old_height);

    let width_scale = new_width as f64 / old_width.max(1) as f64;
    let height_scale = new_height as f64 / old_height.max(1) as f64;

    let half_width = (width_scale * (clamped_x2 - clamped_x1) as f64) / 2.0;
    let half_height = (height_scale * (clamped_y2 - clamped_y1) as f64) / 2.0;

    let middle_x = (width_scale * clamped_x1 as f64 + half_width).round() as i32;
    let middle_y = (height_scale * clamped_y1 as f64 + half_height).round() as i32;

    let target_half_size = (half_width.max(half_height) * 1.1).floor() as i32;

    let new_half_size = [
        middle_x - (middle_x - target_half_size).max(0),
        middle_y - (middle_y - target_half_size).max(0),
        ((new_width as i32 - 1).min(middle_x + target_half_size) - middle_x).max(0),
        ((new_height as i32 - 1).min(middle_y + target_half_size) - middle_y).max(0),
    ]
    .into_iter()
    .min()
    .unwrap_or(0)
    .max(1);

    CropParams {
        x: middle_x - new_half_size,
        y: middle_y - new_half_size,
        width: (new_half_size * 2).max(1) as u32,
        height: (new_half_size * 2).max(1) as u32,
    }
}

fn crop_image(image: DynamicImage, params: &CropParams) -> DynamicImage {
    let (img_w, img_h) = image.dimensions();
    let x = params.x.max(0) as u32;
    let y = params.y.max(0) as u32;
    let width = params.width.min(img_w.saturating_sub(x)).max(1);
    let height = params.height.min(img_h.saturating_sub(y)).max(1);
    crop_imm(&image, x, y, width, height).to_image().into()
}

fn rotate_image(image: DynamicImage, angle: i64) -> DynamicImage {
    match angle.rem_euclid(360) {
        90 | -270 => rotate90(&image).into(),
        180 | -180 => rotate180(&image).into(),
        270 | -90 => rotate270(&image).into(),
        _ => image,
    }
}

fn mirror_image(mut image: DynamicImage, axis: &str) -> DynamicImage {
    match axis {
        "horizontal" => {
            flip_vertical_in_place(&mut image);
        }
        "vertical" => {
            flip_horizontal_in_place(&mut image);
        }
        _ => {}
    }
    image
}

pub fn apply_exif_orientation(image: DynamicImage, orientation: Option<&str>) -> DynamicImage {
    let Some(value) = orientation.and_then(|v| v.parse::<i32>().ok()) else {
        return image;
    };

    match value {
        2 => mirror_image(image, "vertical"),
        3 => rotate_image(image, 180),
        4 => mirror_image(image, "horizontal"),
        5 => mirror_image(rotate_image(image, 270), "vertical"),
        6 => rotate_image(image, 90),
        7 => mirror_image(rotate_image(image, 90), "vertical"),
        8 => rotate_image(image, 270),
        _ => image,
    }
}
