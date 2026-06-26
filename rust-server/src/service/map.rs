use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::album;
use crate::models::db::auth_permission::Permission;
use crate::models::db::map::{self, MapMarkerSearch};
use crate::models::db::partner;
use crate::models::dto::auth::AuthDto;
use crate::models::response::response::ErrorResp;
use crate::service::access::require_album_access;
use crate::utils::permission::require_permission;
use crate::utils::query::parse_query_bool;

#[derive(Clone)]
pub struct MapService {
    pool: PgPool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapMarkerResponse {
    pub id: Uuid,
    pub lat: f64,
    pub lon: f64,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapReverseGeocodeResponse {
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MapMarkerQuery {
    pub is_archived: Option<String>,
    pub is_favorite: Option<String>,
    pub file_created_after: Option<DateTime<Utc>>,
    pub file_created_before: Option<DateTime<Utc>>,
    pub with_partners: Option<String>,
    pub with_shared_albums: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapReverseGeocodeQuery {
    pub lat: f64,
    pub lon: f64,
}

impl MapService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_map_markers(
        &self,
        auth: &AuthDto,
        query: &MapMarkerQuery,
    ) -> Result<Vec<MapMarkerResponse>, ErrorResp> {
        require_permission(auth, Permission::MapRead)?;

        let mut owner_ids = vec![auth.user.id];
        if query.with_partners.as_deref().and_then(parse_query_bool).unwrap_or(false) {
            owner_ids.extend(partner::get_timeline_partner_ids(&self.pool, &auth.user.id).await?);
        }

        let album_ids = if query
            .with_shared_albums
            .as_deref()
            .and_then(parse_query_bool)
            .unwrap_or(false)
        {
            album::list_accessible_album_ids(&self.pool, &auth.user.id, None, None).await?
        } else {
            vec![]
        };

        let rows = map::get_map_markers(
            &self.pool,
            &MapMarkerSearch {
                owner_ids,
                album_ids,
                auth_user_id: auth.user.id,
                is_archived: query.is_archived.as_deref().and_then(parse_query_bool),
                is_favorite: query.is_favorite.as_deref().and_then(parse_query_bool),
                file_created_after: query.file_created_after,
                file_created_before: query.file_created_before,
            },
        )
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| MapMarkerResponse {
                id: row.id,
                lat: row.lat,
                lon: row.lon,
                city: row.city,
                state: row.state,
                country: row.country,
            })
            .collect())
    }

    pub async fn get_album_map_markers(
        &self,
        auth: &AuthDto,
        album_id: &Uuid,
    ) -> Result<Vec<MapMarkerResponse>, ErrorResp> {
        require_album_access(&self.pool, auth, album_id, Permission::AlbumRead).await?;

        if auth
            .shared_link
            .as_ref()
            .is_some_and(|link| !link.show_exif)
        {
            return Ok(vec![]);
        }

        let rows = map::get_album_map_markers(&self.pool, album_id).await?;
        Ok(rows
            .into_iter()
            .map(|row| MapMarkerResponse {
                id: row.id,
                lat: row.lat,
                lon: row.lon,
                city: row.city,
                state: row.state,
                country: row.country,
            })
            .collect())
    }

    pub async fn reverse_geocode(
        &self,
        query: &MapReverseGeocodeQuery,
    ) -> Result<Vec<MapReverseGeocodeResponse>, ErrorResp> {
        if let Some(place) =
            map::reverse_geocode_places(&self.pool, query.lat, query.lon).await?
        {
            return Ok(vec![MapReverseGeocodeResponse {
                city: place.city,
                state: place.state,
                country: place.country_code,
            }]);
        }

        if let Some(country) =
            map::reverse_geocode_country(&self.pool, query.lat, query.lon).await?
        {
            return Ok(vec![MapReverseGeocodeResponse {
                city: None,
                state: None,
                country: Some(country.admin),
            }]);
        }

        Ok(vec![MapReverseGeocodeResponse {
            city: None,
            state: None,
            country: None,
        }])
    }
}
