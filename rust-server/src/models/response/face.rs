use serde::Serialize;
use uuid::Uuid;

use crate::models::db::asset_edit::AssetEditRow;
use crate::models::db::face::AssetFaceWithPersonRow;
use crate::models::response::search::PersonResponse;
use crate::utils::transform::{transform_face_bounding_box, FaceBoundingBox, ImageDimensions};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetFaceResponse {
    pub id: Uuid,
    pub image_height: i32,
    pub image_width: i32,
    pub bounding_box_x1: i32,
    pub bounding_box_x2: i32,
    pub bounding_box_y1: i32,
    pub bounding_box_y2: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    pub person: Option<PersonResponse>,
}

pub fn map_asset_face(row: &AssetFaceWithPersonRow, auth_user_id: &Uuid) -> AssetFaceResponse {
    map_asset_face_with_edits(row, auth_user_id, &[], ImageDimensions { width: 0, height: 0 })
}

pub fn map_asset_face_with_edits(
    row: &AssetFaceWithPersonRow,
    auth_user_id: &Uuid,
    edits: &[AssetEditRow],
    image_dimensions: ImageDimensions,
) -> AssetFaceResponse {
    let bbox = if edits.is_empty() {
        FaceBoundingBox {
            bounding_box_x1: row.bounding_box_x1,
            bounding_box_y1: row.bounding_box_y1,
            bounding_box_x2: row.bounding_box_x2,
            bounding_box_y2: row.bounding_box_y2,
            image_width: row.image_width,
            image_height: row.image_height,
        }
    } else {
        transform_face_bounding_box(
            FaceBoundingBox {
                bounding_box_x1: row.bounding_box_x1,
                bounding_box_y1: row.bounding_box_y1,
                bounding_box_x2: row.bounding_box_x2,
                bounding_box_y2: row.bounding_box_y2,
                image_width: row.image_width,
                image_height: row.image_height,
            },
            edits,
            image_dimensions,
        )
    };

    AssetFaceResponse {
        id: row.id,
        image_height: bbox.image_height,
        image_width: bbox.image_width,
        bounding_box_x1: bbox.bounding_box_x1,
        bounding_box_x2: bbox.bounding_box_x2,
        bounding_box_y1: bbox.bounding_box_y1,
        bounding_box_y2: bbox.bounding_box_y2,
        source_type: Some(row.source_type.clone()),
        person: map_face_person(row, auth_user_id),
    }
}

fn map_face_person(row: &AssetFaceWithPersonRow, auth_user_id: &Uuid) -> Option<PersonResponse> {
    if row.person_id.is_none() {
        return None;
    }
    if row.person_owner_id != Some(*auth_user_id) {
        return None;
    }

    Some(PersonResponse {
        id: row.person_id?,
        name: row.person_name.clone().unwrap_or_default(),
        birth_date: row
            .person_birth_date
            .map(|value| value.format("%Y-%m-%d").to_string()),
        thumbnail_path: row.person_thumbnail_path.clone().unwrap_or_default(),
        is_hidden: row.person_is_hidden.unwrap_or(false),
        updated_at: row
            .person_updated_at
            .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        is_favorite: row.person_is_favorite,
        color: row.person_color.clone(),
    })
}
