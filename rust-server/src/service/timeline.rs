use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::timeline::{
    get_timeline_partner_ids, get_time_bucket_json, get_time_buckets, user_has_album_access,
    user_owns_person, user_owns_tag, BoundingBox, TimeBucketItem, TimelineFilter,
};
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::utils::permission::require_permission;
use crate::models::db::auth_permission::Permission;

#[derive(Clone)]
pub struct TimelineService {
    pool: PgPool,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeBucketQuery {
    pub user_id: Option<Uuid>,
    pub album_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub tag_id: Option<Uuid>,
    pub is_favorite: Option<String>,
    pub is_trashed: Option<String>,
    pub with_stacked: Option<String>,
    pub with_partners: Option<String>,
    pub order: Option<String>,
    pub order_by: Option<String>,
    pub visibility: Option<String>,
    pub with_coordinates: Option<String>,
    pub bbox: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeBucketAssetQuery {
    #[serde(flatten)]
    pub base: TimeBucketQuery,
    pub time_bucket: String,
}

impl TimelineService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_time_buckets(
        &self,
        auth: &AuthDto,
        query: &TimeBucketQuery,
    ) -> Result<Vec<TimeBucketItem>, ErrorResp> {
        let filter = self.build_filter(auth, query).await?;
        get_time_buckets(&self.pool, &filter)
            .await
            .map_err(ErrorResp::from)
    }

    pub async fn get_time_bucket(
        &self,
        auth: &AuthDto,
        query: &TimeBucketAssetQuery,
    ) -> Result<String, ErrorResp> {
        let filter = self.build_filter(auth, &query.base).await?;
        let include_exif = auth.shared_link.as_ref().is_none_or(|sl| sl.show_exif);
        let include_coordinates = parse_bool(&query.base.with_coordinates).unwrap_or(false)
            && include_exif;

        let json = get_time_bucket_json(
            &self.pool,
            auth.user.id,
            &query.time_bucket,
            &filter,
            include_exif,
            include_coordinates,
        )
        .await?;

        Ok(if json.is_empty() { "{}".to_string() } else { json })
    }

    async fn build_filter(
        &self,
        auth: &AuthDto,
        query: &TimeBucketQuery,
    ) -> Result<TimelineFilter, ErrorResp> {
        self.validate_query(auth, query)?;

        let mut owner_ids = Vec::new();
        let mut album_id = query.album_id;
        let mut shared_link_id = None;

        if let Some(shared_link) = &auth.shared_link {
            if album_id.is_none() {
                if let Some(link_album) = &shared_link.album_id {
                    album_id = Uuid::parse_str(link_album).ok();
                } else {
                    shared_link_id = Uuid::parse_str(&shared_link.id).ok();
                }
            }
        } else if let Some(album_id) = album_id {
            self.require_album_read(auth, &album_id).await?;
        } else {
            let user_id = query.user_id.unwrap_or(auth.user.id);
            self.require_timeline_read(auth, &user_id).await?;
            owner_ids.push(user_id);

            if parse_bool(&query.with_partners).unwrap_or(false) {
                let partners = get_timeline_partner_ids(&self.pool, &user_id).await?;
                owner_ids.extend(partners);
            }
        }

        if let Some(tag_id) = query.tag_id {
            require_permission(auth, Permission::TagRead)?;
            if auth.shared_link.is_none() && !user_owns_tag(&self.pool, &auth.user.id, &tag_id).await? {
                return Err(ErrorResp::BadRequest(
                    "Not found or no tag.read access".to_string(),
                ));
            }
        }

        let person_id = query.person_id;
        if let Some(person_id) = person_id {
            if auth.shared_link.is_some() {
                return Err(ErrorResp::BadRequest(
                    "personId filter is not supported for shared links".to_string(),
                ));
            }
            require_permission(auth, Permission::PersonRead)?;
            if !user_owns_person(&self.pool, &auth.user.id, &person_id).await? {
                return Err(ErrorResp::BadRequest(
                    "Not found or no person.read access".to_string(),
                ));
            }
        }

        let bbox = query
            .bbox
            .as_deref()
            .map(parse_bbox)
            .transpose()?;

        let is_trashed = parse_bool(&query.is_trashed).unwrap_or(false);
        let visibility = query.visibility.clone();

        if visibility.as_deref() == Some("locked") {
            let elevated = auth
                .session
                .as_ref()
                .is_some_and(|s| s.has_elevated_permission);
            if !elevated {
                return Err(ErrorResp::Forbidden("Forbidden".to_string()));
            }
        }

        if visibility.as_deref() == Some("archive") {
            if auth.shared_link.is_none() {
                require_permission(auth, Permission::ArchiveRead)?;
            }
            if let Some(user_id) = query.user_id.or(Some(auth.user.id)) {
                if auth.shared_link.is_none() {
                    self.require_timeline_read(auth, &user_id).await?;
                }
            }
        }

        Ok(TimelineFilter {
            owner_ids,
            album_id,
            tag_id: query.tag_id,
            person_id,
            bbox,
            is_favorite: parse_bool(&query.is_favorite),
            is_trashed,
            visibility,
            use_default_visibility: query.visibility.is_none(),
            with_stacked: parse_bool(&query.with_stacked).unwrap_or(false),
            shared_link_id,
            order_by_taken_at: query.order_by.as_deref() != Some("createdAt"),
            order_desc: query.order.as_deref() != Some("asc"),
        })
    }

    fn validate_query(&self, auth: &AuthDto, query: &TimeBucketQuery) -> Result<(), ErrorResp> {
        if parse_bool(&query.with_partners).unwrap_or(false) {
            let blocked = query.visibility.as_deref() == Some("locked")
                || query.visibility.as_deref() == Some("archive")
                || query.visibility.is_none()
                || query.is_favorite.is_some()
                || parse_bool(&query.is_trashed).unwrap_or(false);

            if blocked {
                return Err(ErrorResp::BadRequest(
                    "withPartners is only supported for non-archived, non-trashed, non-favorited, non-locked assets"
                        .to_string(),
                ));
            }
        }

        if auth.shared_link.as_ref().is_some_and(|sl| !sl.show_exif) {
            // coordinates suppressed in get_time_bucket via include_coordinates
        }

        Ok(())
    }

    async fn require_album_read(&self, auth: &AuthDto, album_id: &Uuid) -> Result<(), ErrorResp> {
        if let Some(shared_link) = &auth.shared_link {
            if shared_link.album_id.as_deref() == Some(&album_id.to_string()) {
                return Ok(());
            }
            return Err(ErrorResp::BadRequest(
                "Not found or no album.read access".to_string(),
            ));
        }

        require_permission(auth, Permission::AlbumRead)?;
        if user_has_album_access(&self.pool, &auth.user.id, album_id).await? {
            Ok(())
        } else {
            Err(ErrorResp::BadRequest(
                "Not found or no album.read access".to_string(),
            ))
        }
    }

    async fn require_timeline_read(&self, auth: &AuthDto, user_id: &Uuid) -> Result<(), ErrorResp> {
        if auth.user.id == *user_id {
            return Ok(());
        }

        require_permission(auth, Permission::TimelineRead)?;
        let partners = get_timeline_partner_ids(&self.pool, &auth.user.id).await?;
        if partners.contains(user_id) {
            Ok(())
        } else {
            Err(ErrorResp::BadRequest(
                "Not found or no timeline.read access".to_string(),
            ))
        }
    }
}

fn parse_bool(value: &Option<String>) -> Option<bool> {
    value
        .as_deref()
        .and_then(crate::utils::query::parse_query_bool)
}

fn parse_bbox(value: &str) -> Result<BoundingBox, ErrorResp> {
    let parts: Vec<&str> = value.split(',').collect();
    if parts.len() != 4 {
        return Err(ErrorResp::BadRequest(
            "bbox must have 4 comma-separated numbers: west,south,east,north".to_string(),
        ));
    }

    let west = parts[0]
        .trim()
        .parse::<f64>()
        .map_err(|_| ErrorResp::BadRequest("bbox parts must be valid numbers".to_string()))?;
    let south = parts[1]
        .trim()
        .parse::<f64>()
        .map_err(|_| ErrorResp::BadRequest("bbox parts must be valid numbers".to_string()))?;
    let east = parts[2]
        .trim()
        .parse::<f64>()
        .map_err(|_| ErrorResp::BadRequest("bbox parts must be valid numbers".to_string()))?;
    let north = parts[3]
        .trim()
        .parse::<f64>()
        .map_err(|_| ErrorResp::BadRequest("bbox parts must be valid numbers".to_string()))?;

    Ok(BoundingBox {
        west,
        south,
        east,
        north,
    })
}
