use chrono::{NaiveDate, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::asset_edit;
use crate::models::db::auth_permission::Permission;
use crate::models::db::face::{self, CreateAssetFaceData};
use crate::models::db::person::{self, PersonListFilter};
use crate::models::db::user_metadata::UserMetadataPO;
use crate::models::dto::auth::AuthDto;
use crate::models::response::face::{AssetFaceResponse, map_asset_face_with_edits};
use crate::models::response::response::ErrorResp;
use crate::models::response::search::{PersonResponse, map_person};
use crate::service::access::require_assets_access;
use crate::service::album::{BulkIdErrorReason, BulkIdResponse};
use crate::service::job::JobService;
use crate::service::media::visibility::asset_dimensions_from_exif;
use crate::utils::file_response::{FileResponse, file_response, guess_mime};
use crate::utils::permission::require_permission;
use crate::utils::preferences::resolve_preferences;
use crate::utils::query::parse_query_bool;
use crate::utils::transform::{ImageDimensions, Point, transform_points};

#[derive(Clone)]
pub struct PersonService {
    pool: PgPool,
    jobs: JobService,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeopleResponse {
    pub total: i64,
    pub hidden: i64,
    pub people: Vec<PersonResponse>,
    pub has_next_page: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonStatisticsResponse {
    pub assets: i64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonSearchQuery {
    pub with_hidden: Option<String>,
    pub closest_person_id: Option<Uuid>,
    pub closest_asset_id: Option<Uuid>,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_size")]
    pub size: i64,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersonCreateReq {
    pub name: Option<String>,
    pub birth_date: Option<String>,
    pub is_hidden: Option<bool>,
    pub is_favorite: Option<bool>,
    pub color: Option<String>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersonUpdateReq {
    pub name: Option<String>,
    pub birth_date: Option<String>,
    pub is_hidden: Option<bool>,
    pub is_favorite: Option<bool>,
    pub color: Option<String>,
    pub feature_face_asset_id: Option<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeopleUpdateItemReq {
    pub id: Uuid,
    #[serde(flatten)]
    pub update: PersonUpdateReq,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeopleUpdateReq {
    pub people: Vec<PeopleUpdateItemReq>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkIdsReq {
    pub ids: Vec<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetFaceUpdateItemReq {
    pub person_id: Uuid,
    pub asset_id: Uuid,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetFaceUpdateReq {
    pub data: Vec<AssetFaceUpdateItemReq>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePersonReq {
    pub ids: Vec<Uuid>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceQuery {
    pub id: Uuid,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceReassignReq {
    pub id: Uuid,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetFaceCreateReq {
    pub person_id: Uuid,
    pub asset_id: Uuid,
    pub image_width: i32,
    pub image_height: i32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetFaceDeleteReq {
    pub force: bool,
}

fn default_page() -> i64 {
    1
}

fn default_size() -> i64 {
    500
}

impl PersonService {
    pub fn new(pool: PgPool, jobs: JobService) -> Self {
        Self { pool, jobs }
    }

    pub async fn get_all(
        &self,
        auth: &AuthDto,
        query: &PersonSearchQuery,
    ) -> Result<PeopleResponse, ErrorResp> {
        require_permission(auth, Permission::PersonRead)?;
        let with_hidden = query
            .with_hidden
            .as_deref()
            .and_then(parse_query_bool)
            .unwrap_or(false);
        let page = query.page.max(1);
        let size = query.size.clamp(1, 1000);
        let offset = (page - 1) * size;

        let mut closest_face_id = query.closest_asset_id;
        if let Some(closest_person_id) = query.closest_person_id {
            let person = person::get_by_id_for_owner(&self.pool, &auth.user.id, &closest_person_id)
                .await?
                .ok_or_else(|| ErrorResp::NotFound("Person not found".to_string()))?;
            let face_id = person::get_face_asset_id(&self.pool, &auth.user.id, &person.id)
                .await?
                .ok_or_else(|| ErrorResp::NotFound("Person not found".to_string()))?;
            closest_face_id = Some(face_id);
        }

        let minimum_faces = self.get_minimum_faces(&auth.user.id).await?;
        let filter = PersonListFilter {
            with_hidden,
            minimum_faces,
            closest_face_id,
            limit: size + 1,
            offset,
        };

        let counts = person::count_for_user(&self.pool, &auth.user.id).await?;
        let mut items = person::list_for_user(&self.pool, &auth.user.id, &filter).await?;

        let has_next_page = items.len() as i64 > size;
        if has_next_page {
            items.pop();
        }

        Ok(PeopleResponse {
            total: counts.total,
            hidden: counts.hidden,
            people: items.iter().map(map_person).collect(),
            has_next_page,
        })
    }

    pub async fn get(&self, auth: &AuthDto, id: &Uuid) -> Result<PersonResponse, ErrorResp> {
        require_permission(auth, Permission::PersonRead)?;
        self.find_or_fail(auth, id).await
    }

    pub async fn get_statistics(
        &self,
        auth: &AuthDto,
        id: &Uuid,
    ) -> Result<PersonStatisticsResponse, ErrorResp> {
        require_permission(auth, Permission::PersonStatistics)?;
        self.require_person_owner(auth, &[*id]).await?;
        let stats = person::get_statistics(&self.pool, id).await?;
        Ok(PersonStatisticsResponse {
            assets: stats.assets,
        })
    }

    pub async fn get_thumbnail(
        &self,
        auth: &AuthDto,
        id: &Uuid,
    ) -> Result<axum::response::Response, ErrorResp> {
        require_permission(auth, Permission::PersonRead)?;
        let row = person::get_by_id_for_owner(&self.pool, &auth.user.id, id)
            .await?
            .ok_or_else(|| ErrorResp::NotFound("Person not found".to_string()))?;

        if row.thumbnail_path.is_empty() {
            return Err(ErrorResp::NotFound("Person not found".to_string()));
        }

        let path = row.thumbnail_path.clone();
        file_response(FileResponse {
            path,
            content_type: guess_mime(&row.thumbnail_path),
            file_name: None,
            cache_control: Some("private, no-cache, no-transform".to_string()),
        })
        .await
    }

    pub async fn create(
        &self,
        auth: &AuthDto,
        dto: &PersonCreateReq,
    ) -> Result<PersonResponse, ErrorResp> {
        require_permission(auth, Permission::PersonCreate)?;
        let birth_date = parse_birth_date(dto.birth_date.as_deref())?;
        let color = parse_color(dto.color.as_deref())?;
        let row = person::create(
            &self.pool,
            &auth.user.id,
            dto.name.as_deref(),
            birth_date,
            dto.is_hidden,
            dto.is_favorite,
            color.as_deref(),
        )
        .await?;
        Ok(map_person(&row))
    }

    pub async fn update(
        &self,
        auth: &AuthDto,
        id: &Uuid,
        dto: &PersonUpdateReq,
    ) -> Result<PersonResponse, ErrorResp> {
        require_permission(auth, Permission::PersonUpdate)?;
        self.require_person_owner(auth, &[*id]).await?;

        let birth_date_update = match dto.birth_date.as_deref() {
            None => None,
            Some(value) => Some(parse_birth_date(Some(value))?),
        };

        let color_update = match dto.color.as_deref() {
            None => None,
            Some(value) => Some(parse_color(Some(value))?),
        };

        let update_face_asset_id = if let Some(asset_id) = dto.feature_face_asset_id {
            require_assets_access(&self.pool, auth, &[asset_id], Permission::AssetRead).await?;
            let face_id = person::get_face_id_for_feature_update(&self.pool, id, &asset_id)
                .await?
                .ok_or_else(|| {
                    ErrorResp::BadRequest(
                        "Invalid assetId for feature face or asset is offline".to_string(),
                    )
                })?;
            Some(Some(face_id))
        } else {
            None
        };

        let row = person::update(
            &self.pool,
            id,
            &auth.user.id,
            dto.name.as_deref(),
            birth_date_update,
            dto.is_hidden,
            dto.is_favorite,
            color_update.as_ref().map(|value| value.as_deref()),
            update_face_asset_id,
        )
        .await?;

        if update_face_asset_id.is_some() {
            self.jobs
                .queue_person_generate_thumbnail(&auth.user.id, id)
                .await?;
        }

        Ok(map_person(&row))
    }

    pub async fn update_all(
        &self,
        auth: &AuthDto,
        dto: &PeopleUpdateReq,
    ) -> Result<Vec<BulkIdResponse>, ErrorResp> {
        require_permission(auth, Permission::PersonUpdate)?;
        let mut results = Vec::with_capacity(dto.people.len());
        for item in &dto.people {
            match self.update(auth, &item.id, &item.update).await {
                Ok(_) => results.push(BulkIdResponse {
                    id: item.id,
                    success: true,
                    error: None,
                }),
                Err(err) => results.push(BulkIdResponse {
                    id: item.id,
                    success: false,
                    error: Some(match err {
                        ErrorResp::BadRequest(_) => BulkIdErrorReason::NotFound,
                        ErrorResp::Forbidden(_) => BulkIdErrorReason::NoPermission,
                        _ => BulkIdErrorReason::Unknown,
                    }),
                }),
            }
        }
        Ok(results)
    }

    pub async fn delete(&self, auth: &AuthDto, id: &Uuid) -> Result<(), ErrorResp> {
        self.delete_all(auth, &BulkIdsReq { ids: vec![*id] }).await
    }

    pub async fn delete_all(&self, auth: &AuthDto, dto: &BulkIdsReq) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::PersonDelete)?;
        self.require_person_owner(auth, &dto.ids).await?;
        self.unlink_people_thumbnails(&auth.user.id, &dto.ids)
            .await?;
        person::delete_for_owner(&self.pool, &auth.user.id, &dto.ids).await?;
        person::delete_empty_groups(&self.pool).await?;
        Ok(())
    }

    pub async fn reassign_faces(
        &self,
        auth: &AuthDto,
        target_id: &Uuid,
        dto: &AssetFaceUpdateReq,
    ) -> Result<Vec<PersonResponse>, ErrorResp> {
        require_permission(auth, Permission::PersonUpdate)?;
        require_permission(auth, Permission::PersonReassign)?;
        self.require_person_owner(auth, &[*target_id]).await?;

        let target_face_asset_id = person::get_face_asset_id(&self.pool, &auth.user.id, target_id).await?;
        let mut change_feature_photo = Vec::new();
        if target_face_asset_id.is_none() {
            change_feature_photo.push(*target_id);
        }

        for item in &dto.data {
            self.require_person_owner(auth, &[item.person_id]).await?;
            if let Some(face_id) =
                person::get_face_id_for_asset(&self.pool, &item.person_id, &item.asset_id).await?
            {
                if person::get_face_asset_id(&self.pool, &auth.user.id, &item.person_id).await? == Some(face_id) {
                    change_feature_photo.push(item.person_id);
                }
                person::reassign_face(&self.pool, &face_id, target_id).await?;
            }
        }

        change_feature_photo.sort_unstable();
        change_feature_photo.dedup();
        if !change_feature_photo.is_empty() {
            self.create_new_feature_photo(&auth.user.id, &change_feature_photo)
                .await?;
        }

        Ok(vec![self.find_or_fail(auth, target_id).await?])
    }

    pub async fn merge(
        &self,
        auth: &AuthDto,
        target_id: &Uuid,
        dto: &MergePersonReq,
    ) -> Result<Vec<BulkIdResponse>, ErrorResp> {
        require_permission(auth, Permission::PersonUpdate)?;
        require_permission(auth, Permission::PersonMerge)?;
        self.require_person_owner(auth, &[*target_id]).await?;

        let mut primary = person::get_by_id_for_owner(&self.pool, &auth.user.id, target_id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Person not found".to_string()))?;

        let mut results = Vec::new();
        for merge_id in &dto.ids {
            if merge_id == target_id {
                return Err(ErrorResp::BadRequest(
                    "Cannot merge a person into themselves".to_string(),
                ));
            }

            if !person::owner_owns_people(&self.pool, &auth.user.id, &[*merge_id]).await? {
                results.push(BulkIdResponse {
                    id: *merge_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NoPermission),
                });
                continue;
            }

            let Some(merge_person) =
                person::get_by_id_for_owner(&self.pool, &auth.user.id, merge_id).await?
            else {
                results.push(BulkIdResponse {
                    id: *merge_id,
                    success: false,
                    error: Some(BulkIdErrorReason::NotFound),
                });
                continue;
            };

            let mut name = None;
            let mut birth_date: Option<Option<chrono::NaiveDate>> = None;
            if primary.name.is_empty() && !merge_person.name.is_empty() {
                name = Some(merge_person.name.as_str());
            }
            if primary.birth_date.is_none() && merge_person.birth_date.is_some() {
                birth_date = Some(merge_person.birth_date);
            }
            if name.is_some() || birth_date.is_some() {
                primary = person::update(
                    &self.pool,
                    target_id,
                    &auth.user.id,
                    name,
                    birth_date,
                    None,
                    None,
                    None,
                    None,
                )
                .await?;
            }

            person::reassign_faces_by_person(&self.pool, merge_id, target_id, &auth.user.id)
                .await?;
            if !merge_person.thumbnail_path.is_empty() {
                let _ = tokio::fs::remove_file(&merge_person.thumbnail_path).await;
            }
            person::delete_for_owner(&self.pool, &auth.user.id, &[*merge_id]).await?;
            results.push(BulkIdResponse {
                id: *merge_id,
                success: true,
                error: None,
            });
        }

        if results.iter().any(|result| result.success) {
            if let Err(err) = person::vacuum_faces(&self.pool, false).await {
                tracing::error!("person merge: vacuum_faces failed: {err}");
            }
        }

        Ok(results)
    }

    pub async fn create_face(
        &self,
        auth: &AuthDto,
        dto: &AssetFaceCreateReq,
    ) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::FaceCreate)?;
        require_assets_access(&self.pool, auth, &[dto.asset_id], Permission::AssetUpdate).await?;
        self.require_person_owner(auth, &[dto.person_id]).await?;

        let mut image_width = dto.image_width;
        let mut image_height = dto.image_height;
        let mut x1 = dto.x;
        let mut y1 = dto.y;
        let mut x2 = dto.x + dto.width;
        let mut y2 = dto.y + dto.height;

        let edits = asset_edit::list_by_asset(&self.pool, &dto.asset_id).await?;

        if !edits.is_empty() {
            let (asset_width, _asset_height, exif_width, exif_height) =
                face::get_asset_scale_for_face(&self.pool, &dto.asset_id)
                    .await?
                    .ok_or_else(|| {
                        ErrorResp::BadRequest("Asset does not have valid dimensions".to_string())
                    })?;

            if dto.image_width <= 0 {
                return Err(ErrorResp::BadRequest(
                    "Asset does not have valid dimensions".to_string(),
                ));
            }

            let scale_factor = asset_width as f64 / dto.image_width as f64;
            let top_left = Point {
                x: dto.x as f64 * scale_factor,
                y: dto.y as f64 * scale_factor,
            };
            let bottom_right = Point {
                x: (dto.x + dto.width) as f64 * scale_factor,
                y: (dto.y + dto.height) as f64 * scale_factor,
            };

            let (transformed, _, _) = transform_points(
                &[top_left, bottom_right],
                &edits,
                ImageDimensions {
                    width: asset_width,
                    height: _asset_height,
                },
                true,
            );

            x1 = transformed[0].x.min(transformed[1].x).round() as i32;
            y1 = transformed[0].y.min(transformed[1].y).round() as i32;
            x2 = transformed[0].x.max(transformed[1].x).round() as i32;
            y2 = transformed[0].y.max(transformed[1].y).round() as i32;
            image_width = exif_width;
            image_height = exif_height;
        }

        face::create_asset_face(
            &self.pool,
            &CreateAssetFaceData {
                person_id: dto.person_id,
                asset_id: dto.asset_id,
                image_width,
                image_height,
                bounding_box_x1: x1.min(x2),
                bounding_box_y1: y1.min(y2),
                bounding_box_x2: x1.max(x2),
                bounding_box_y2: y1.max(y2),
            },
        )
        .await?;

        if face::get_person_face_asset_id(&self.pool, &auth.user.id, &dto.person_id)
            .await?
            .is_none()
        {
            self.create_new_feature_photo(&auth.user.id, &[dto.person_id])
                .await?;
        }

        Ok(())
    }

    pub async fn get_faces_by_asset(
        &self,
        auth: &AuthDto,
        asset_id: &Uuid,
    ) -> Result<Vec<AssetFaceResponse>, ErrorResp> {
        require_permission(auth, Permission::FaceRead)?;
        require_assets_access(&self.pool, auth, &[*asset_id], Permission::AssetRead).await?;

        let rows = face::get_faces_by_asset(&self.pool, asset_id).await?;
        let edits = asset_edit::list_by_asset(&self.pool, asset_id).await?;
        let asset = face::get_asset_for_faces(&self.pool, asset_id).await?;
        let exif_dims = asset
            .as_ref()
            .map(|value| {
                asset_dimensions_from_exif(
                    value.exif_image_width,
                    value.exif_image_height,
                    value.orientation.as_deref(),
                )
            })
            .unwrap_or(crate::service::media::visibility::ImageDimensions {
                width: 0.0,
                height: 0.0,
            });
        let image_dimensions = ImageDimensions {
            width: exif_dims.width as i32,
            height: exif_dims.height as i32,
        };

        Ok(rows
            .iter()
            .map(|row| map_asset_face_with_edits(row, &auth.user.id, &edits, image_dimensions))
            .collect())
    }

    pub async fn reassign_face_by_id(
        &self,
        auth: &AuthDto,
        person_id: &Uuid,
        face_id: &Uuid,
    ) -> Result<PersonResponse, ErrorResp> {
        require_permission(auth, Permission::FaceUpdate)?;
        require_permission(auth, Permission::PersonUpdate)?;
        self.require_person_owner(auth, &[*person_id]).await?;
        self.require_face_owner(auth, face_id).await?;

        let face_row = face::get_face_by_id(&self.pool, face_id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Face not found".to_string()))?;

        person::reassign_face(&self.pool, face_id, person_id).await?;

        let target_face_asset_id = face::get_person_face_asset_id(&self.pool, &auth.user.id, person_id).await?;
        if target_face_asset_id.is_none() {
            self.create_new_feature_photo(&auth.user.id, &[*person_id])
                .await?;
        }

        if let Some(old_person_id) = face_row.person_id {
            if face::get_person_face_asset_id(&self.pool, &auth.user.id, &old_person_id).await? == Some(*face_id) {
                self.create_new_feature_photo(&auth.user.id, &[old_person_id])
                    .await?;
            }
        }

        self.find_or_fail(auth, person_id).await
    }

    pub async fn delete_face(
        &self,
        auth: &AuthDto,
        face_id: &Uuid,
        dto: &AssetFaceDeleteReq,
    ) -> Result<(), ErrorResp> {
        require_permission(auth, Permission::FaceDelete)?;
        self.require_face_owner(auth, face_id).await?;

        if dto.force {
            face::delete_asset_face(&self.pool, face_id).await?;
        } else {
            face::soft_delete_asset_face(&self.pool, face_id).await?;
        }
        Ok(())
    }

    async fn require_face_owner(&self, auth: &AuthDto, face_id: &Uuid) -> Result<(), ErrorResp> {
        if !face::owner_owns_face(&self.pool, &auth.user.id, face_id).await? {
            return Err(ErrorResp::BadRequest(
                "Not found or no face access".to_string(),
            ));
        }
        Ok(())
    }

    async fn create_new_feature_photo(
        &self,
        owner_id: &Uuid,
        person_ids: &[Uuid],
    ) -> Result<(), ErrorResp> {
        for person_id in person_ids {
            if let Some(face_id) = face::get_random_face_id(&self.pool, person_id).await? {
                face::set_person_face_asset_id(&self.pool, owner_id, person_id, Some(face_id)).await?;
                self.jobs
                    .queue_person_generate_thumbnail(owner_id, person_id)
                    .await?;
            }
        }
        Ok(())
    }

    async fn get_minimum_faces(&self, user_id: &Uuid) -> Result<i32, ErrorResp> {
        let stored = UserMetadataPO::get_preferences_json(&self.pool, user_id)
            .await
            .map_err(ErrorResp::from)?;
        let prefs = resolve_preferences(stored);
        Ok(prefs
            .get("people")
            .and_then(|value| value.get("minimumFaces"))
            .and_then(|value| value.as_i64())
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(3))
    }

    async fn unlink_people_thumbnails(
        &self,
        owner_id: &Uuid,
        ids: &[Uuid],
    ) -> Result<(), ErrorResp> {
        let paths = person::get_thumbnail_paths_for_owner(&self.pool, owner_id, ids)
            .await
            .map_err(ErrorResp::from)?;
        for path in paths {
            let _ = tokio::fs::remove_file(path).await;
        }
        Ok(())
    }

    async fn find_or_fail(&self, auth: &AuthDto, id: &Uuid) -> Result<PersonResponse, ErrorResp> {
        let row = person::get_by_id_for_owner(&self.pool, &auth.user.id, id)
            .await?
            .ok_or_else(|| ErrorResp::BadRequest("Person not found".to_string()))?;
        Ok(map_person(&row))
    }

    async fn require_person_owner(&self, auth: &AuthDto, ids: &[Uuid]) -> Result<(), ErrorResp> {
        if !person::owner_owns_people(&self.pool, &auth.user.id, ids).await? {
            return Err(ErrorResp::BadRequest(
                "Not found or no person access".to_string(),
            ));
        }
        Ok(())
    }
}

fn parse_birth_date(value: Option<&str>) -> Result<Option<NaiveDate>, ErrorResp> {
    match value {
        None => Ok(None),
        Some("") => Ok(None),
        Some(raw) => {
            let date = NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .map_err(|_| ErrorResp::BadRequest("Invalid birth date".to_string()))?;
            if date > Utc::now().date_naive() {
                return Err(ErrorResp::BadRequest(
                    "Birth date cannot be in the future".to_string(),
                ));
            }
            Ok(Some(date))
        }
    }
}

fn parse_color(value: Option<&str>) -> Result<Option<String>, ErrorResp> {
    match value {
        None => Ok(None),
        Some("") => Ok(None),
        Some(raw) => {
            let hex = raw.trim();
            if !is_valid_hex_color(hex) {
                return Err(ErrorResp::BadRequest("Invalid color".to_string()));
            }
            Ok(Some(if hex.starts_with('#') {
                hex.to_string()
            } else {
                format!("#{hex}")
            }))
        }
    }
}

fn is_valid_hex_color(value: &str) -> bool {
    let digits = value.strip_prefix('#').unwrap_or(value);
    matches!(digits.len(), 3 | 4 | 6 | 8) && digits.chars().all(|ch| ch.is_ascii_hexdigit())
}
