use crate::models::db::asset_edit::AssetEditRow;
use crate::service::media::edits::parse_crop;

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageDimensions {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct FaceBoundingBox {
    pub bounding_box_x1: i32,
    pub bounding_box_y1: i32,
    pub bounding_box_x2: i32,
    pub bounding_box_y2: i32,
    pub image_width: i32,
    pub image_height: i32,
}

pub fn transform_points(
    points: &[Point],
    edits: &[AssetEditRow],
    starting_dimensions: ImageDimensions,
    inverse: bool,
) -> (Vec<Point>, i32, i32) {
    let mut current_width = starting_dimensions.width;
    let mut current_height = starting_dimensions.height;
    let mut transformed = points.to_vec();

    if !inverse {
        if let Some(crop) = edits.iter().find(|edit| edit.action == "crop") {
            if let Some(params) = parse_crop(&crop.parameters) {
                transformed = transformed
                    .iter()
                    .map(|p| Point {
                        x: p.x - params.x as f64,
                        y: p.y - params.y as f64,
                    })
                    .collect();
                current_width = params.width as i32;
                current_height = params.height as i32;
            }
        }
    }

    let edit_sequence: Vec<&AssetEditRow> = if inverse {
        edits.iter().rev().collect()
    } else {
        edits.iter().collect()
    };

    for edit in edit_sequence {
        match edit.action.as_str() {
            "rotate" => {
                let angle_degrees = edit
                    .parameters
                    .get("angle")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let angle_rad = (angle_degrees as f64) * std::f64::consts::PI / 180.0;
                let signed_angle = if inverse { -angle_rad } else { angle_rad };

                let (new_width, new_height) = if angle_degrees == 90 || angle_degrees == 270 {
                    (current_height, current_width)
                } else {
                    (current_width, current_height)
                };

                transformed = transformed
                    .iter()
                    .map(|p| {
                        rotate_point(
                            *p,
                            current_width,
                            current_height,
                            signed_angle,
                            new_width,
                            new_height,
                        )
                    })
                    .collect();
                current_width = new_width;
                current_height = new_height;
            }
            "mirror" => {
                let axis = edit
                    .parameters
                    .get("axis")
                    .and_then(|v| v.as_str())
                    .unwrap_or("horizontal");
                transformed = transformed
                    .iter()
                    .map(|p| mirror_point(*p, current_width, current_height, axis))
                    .collect();
            }
            _ => {}
        }
    }

    if inverse {
        if let Some(crop) = edits.iter().find(|edit| edit.action == "crop") {
            if let Some(params) = parse_crop(&crop.parameters) {
                transformed = transformed
                    .iter()
                    .map(|p| Point {
                        x: p.x + params.x as f64,
                        y: p.y + params.y as f64,
                    })
                    .collect();
            }
        }
    }

    (transformed, current_width, current_height)
}

pub fn transform_face_bounding_box(
    bbox: FaceBoundingBox,
    edits: &[AssetEditRow],
    image_dimensions: ImageDimensions,
) -> FaceBoundingBox {
    if edits.is_empty() {
        return bbox;
    }

    if bbox.image_width == 0 || bbox.image_height == 0 {
        return bbox;
    }

    let scale_x = image_dimensions.width as f64 / bbox.image_width as f64;
    let scale_y = image_dimensions.height as f64 / bbox.image_height as f64;

    let points = [
        Point {
            x: bbox.bounding_box_x1 as f64 * scale_x,
            y: bbox.bounding_box_y1 as f64 * scale_y,
        },
        Point {
            x: bbox.bounding_box_x2 as f64 * scale_x,
            y: bbox.bounding_box_y2 as f64 * scale_y,
        },
    ];

    let (transformed, current_width, current_height) =
        transform_points(&points, edits, image_dimensions, false);

    let p1 = transformed[0];
    let p2 = transformed[1];

    FaceBoundingBox {
        bounding_box_x1: p1.x.min(p2.x).trunc() as i32,
        bounding_box_y1: p1.y.min(p2.y).trunc() as i32,
        bounding_box_x2: p1.x.max(p2.x).trunc() as i32,
        bounding_box_y2: p1.y.max(p2.y).trunc() as i32,
        image_width: current_width,
        image_height: current_height,
    }
}

fn rotate_point(
    point: Point,
    current_width: i32,
    current_height: i32,
    angle_rad: f64,
    new_width: i32,
    new_height: i32,
) -> Point {
    let cos = angle_rad.cos();
    let sin = angle_rad.sin();
    let cx = current_width as f64 / 2.0;
    let cy = current_height as f64 / 2.0;
    let ncx = new_width as f64 / 2.0;
    let ncy = new_height as f64 / 2.0;

    let x = point.x - cx;
    let y = point.y - cy;
    let rx = x * cos - y * sin;
    let ry = x * sin + y * cos;

    Point {
        x: rx + ncx,
        y: ry + ncy,
    }
}

fn mirror_point(point: Point, width: i32, height: i32, axis: &str) -> Point {
    if axis == "horizontal" {
        Point {
            x: point.x,
            y: height as f64 - point.y,
        }
    } else {
        Point {
            x: width as f64 - point.x,
            y: point.y,
        }
    }
}
