#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageDimensions {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug)]
pub struct FaceForVisibility {
    pub id: uuid::Uuid,
    pub bounding_box_x1: f32,
    pub bounding_box_y1: f32,
    pub bounding_box_x2: f32,
    pub bounding_box_y2: f32,
    pub image_width: i32,
    pub image_height: i32,
    pub is_visible: bool,
}

#[derive(Debug)]
pub struct OcrForVisibility {
    pub id: uuid::Uuid,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub x3: f32,
    pub y3: f32,
    pub x4: f32,
    pub y4: f32,
    pub text: String,
    pub is_visible: bool,
}

#[derive(Debug, Default)]
pub struct VisibilityUpdate {
    pub visible_ids: Vec<uuid::Uuid>,
    pub hidden_ids: Vec<uuid::Uuid>,
}

pub fn asset_dimensions_from_exif(
    exif_width: Option<i32>,
    exif_height: Option<i32>,
    orientation: Option<&str>,
) -> ImageDimensions {
    let Some(width) = exif_width.filter(|v| *v > 0) else {
        return ImageDimensions {
            width: 0.0,
            height: 0.0,
        };
    };
    let Some(height) = exif_height.filter(|v| *v > 0) else {
        return ImageDimensions {
            width: 0.0,
            height: 0.0,
        };
    };

    if is_flipped_orientation(orientation) {
        ImageDimensions {
            width: height as f32,
            height: width as f32,
        }
    } else {
        ImageDimensions {
            width: width as f32,
            height: height as f32,
        }
    }
}

fn is_flipped_orientation(orientation: Option<&str>) -> bool {
    let Some(value) = orientation.and_then(|v| v.parse::<i32>().ok()) else {
        return false;
    };
    matches!(value, 5 | 6 | 7 | 8 | -90 | 90)
}

pub fn bounding_box_overlap(box_a: &BoundingBox, box_b: &BoundingBox) -> f32 {
    let overlap_x1 = box_a.x1.max(box_b.x1);
    let overlap_y1 = box_a.y1.max(box_b.y1);
    let overlap_x2 = box_a.x2.min(box_b.x2);
    let overlap_y2 = box_a.y2.min(box_b.y2);

    let overlap_area =
        (overlap_x2 - overlap_x1).max(0.0) * (overlap_y2 - overlap_y1).max(0.0);
    let face_area = (box_a.x2 - box_a.x1) * (box_a.y2 - box_a.y1);
    if face_area <= 0.0 {
        return 0.0;
    }
    overlap_area / face_area
}

fn scale_box(
    bbox: &BoundingBox,
    target: ImageDimensions,
    source: Option<ImageDimensions>,
) -> BoundingBox {
    let source_width = source.map(|d| d.width).unwrap_or(1.0).max(1.0);
    let source_height = source.map(|d| d.height).unwrap_or(1.0).max(1.0);

    BoundingBox {
        x1: (bbox.x1 / source_width) * target.width,
        y1: (bbox.y1 / source_height) * target.height,
        x2: (bbox.x2 / source_width) * target.width,
        y2: (bbox.y2 / source_height) * target.height,
    }
}

pub fn check_face_visibility(
    faces: &[FaceForVisibility],
    original_asset_dimensions: ImageDimensions,
    crop: Option<&BoundingBox>,
) -> VisibilityUpdate {
    let Some(crop) = crop else {
        return VisibilityUpdate {
            visible_ids: faces
                .iter()
                .filter(|face| !face.is_visible)
                .map(|face| face.id)
                .collect(),
            hidden_ids: Vec::new(),
        };
    };

    let mut update = VisibilityUpdate::default();
    for face in faces {
        let scaled_face = scale_box(
            &BoundingBox {
                x1: face.bounding_box_x1,
                y1: face.bounding_box_y1,
                x2: face.bounding_box_x2,
                y2: face.bounding_box_y2,
            },
            original_asset_dimensions,
            Some(ImageDimensions {
                width: face.image_width.max(1) as f32,
                height: face.image_height.max(1) as f32,
            }),
        );

        if bounding_box_overlap(&scaled_face, crop) >= 0.5 {
            update.visible_ids.push(face.id);
        } else {
            update.hidden_ids.push(face.id);
        }
    }
    update
}

pub fn check_ocr_visibility(
    ocrs: &[OcrForVisibility],
    original_asset_dimensions: ImageDimensions,
    crop: Option<&BoundingBox>,
) -> VisibilityUpdate {
    let Some(crop) = crop else {
        return VisibilityUpdate {
            visible_ids: ocrs
                .iter()
                .filter(|ocr| !ocr.is_visible)
                .map(|ocr| ocr.id)
                .collect(),
            hidden_ids: Vec::new(),
        };
    };

    let mut update = VisibilityUpdate::default();
    for ocr in ocrs {
        let ocr_box = scale_box(
            &BoundingBox {
                x1: ocr.x1.min(ocr.x2).min(ocr.x3).min(ocr.x4),
                y1: ocr.y1.min(ocr.y2).min(ocr.y3).min(ocr.y4),
                x2: ocr.x1.max(ocr.x2).max(ocr.x3).max(ocr.x4),
                y2: ocr.y1.max(ocr.y2).max(ocr.y3).max(ocr.y4),
            },
            original_asset_dimensions,
            None,
        );

        if bounding_box_overlap(&ocr_box, crop) >= 0.5 {
            update.visible_ids.push(ocr.id);
        } else {
            update.hidden_ids.push(ocr.id);
        }
    }
    update
}

pub fn visible_ocr_search_text(ocrs: &[OcrForVisibility], visible_ids: &[uuid::Uuid]) -> String {
    ocrs.iter()
        .filter(|ocr| visible_ids.contains(&ocr.id))
        .map(|ocr| ocr.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_without_crop_restores_hidden_faces() {
        let faces = vec![FaceForVisibility {
            id: uuid::Uuid::new_v4(),
            bounding_box_x1: 0.0,
            bounding_box_y1: 0.0,
            bounding_box_x2: 10.0,
            bounding_box_y2: 10.0,
            image_width: 100,
            image_height: 100,
            is_visible: false,
        }];
        let result = check_face_visibility(
            &faces,
            ImageDimensions {
                width: 100.0,
                height: 100.0,
            },
            None,
        );
        assert_eq!(result.visible_ids.len(), 1);
        assert!(result.hidden_ids.is_empty());
    }

    #[test]
    fn face_inside_crop_is_visible() {
        let id = uuid::Uuid::new_v4();
        let faces = vec![FaceForVisibility {
            id,
            bounding_box_x1: 10.0,
            bounding_box_y1: 10.0,
            bounding_box_x2: 30.0,
            bounding_box_y2: 30.0,
            image_width: 100,
            image_height: 100,
            is_visible: true,
        }];
        let crop = BoundingBox {
            x1: 0.0,
            y1: 0.0,
            x2: 50.0,
            y2: 50.0,
        };
        let result = check_face_visibility(
            &faces,
            ImageDimensions {
                width: 100.0,
                height: 100.0,
            },
            Some(&crop),
        );
        assert_eq!(result.visible_ids, vec![id]);
        assert!(result.hidden_ids.is_empty());
    }
}
