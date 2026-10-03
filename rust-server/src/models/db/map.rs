use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct MapMarkerRow {
    pub id: Uuid,
    pub lat: f64,
    pub lon: f64,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, FromRow)]
pub struct ReverseGeocodeRow {
    pub city: Option<String>,
    pub state: Option<String>,
    pub country_code: Option<String>,
}

#[derive(Debug, FromRow)]
pub struct NaturalEarthRow {
    pub admin: String,
    pub admin_a3: String,
}

pub struct MapMarkerSearch {
    pub owner_ids: Vec<Uuid>,
    pub album_ids: Vec<Uuid>,
    pub auth_user_id: Uuid,
    pub is_archived: Option<bool>,
    pub is_favorite: Option<bool>,
    pub file_created_after: Option<DateTime<Utc>>,
    pub file_created_before: Option<DateTime<Utc>>,
}

pub async fn get_album_map_markers(
    pool: &Pool<Postgres>,
    album_id: &Uuid,
) -> Result<Vec<MapMarkerRow>, sqlx::Error> {
    sqlx::query_as::<_, MapMarkerRow>(
        r#"
            SELECT
                asset.id,
                e.latitude as lat,
                e.longitude as lon,
                e.city,
                e.state,
                e.country
            FROM asset
            INNER JOIN asset_exif e ON e."assetId" = asset.id
                AND e.latitude IS NOT NULL
                AND e.longitude IS NOT NULL
            INNER JOIN album_asset aa ON aa."assetId" = asset.id
            WHERE asset."deletedAt" IS NULL
              AND aa."albumId" = $1
            ORDER BY asset."fileCreatedAt" DESC
        "#,
    )
    .bind(album_id)
    .fetch_all(pool)
    .await
}

fn map_marker_sql(search: &MapMarkerSearch) -> (String, bool) {
    let mut sql = String::from(
        r#"
            SELECT
                asset.id,
                e.latitude as lat,
                e.longitude as lon,
                e.city,
                e.state,
                e.country
            FROM asset
            INNER JOIN asset_exif e ON e."assetId" = asset.id
                AND e.latitude IS NOT NULL
                AND e.longitude IS NOT NULL
            WHERE asset."deletedAt" IS NULL
        "#,
    );

    let include_viewer = search.is_archived == Some(true);
    if include_viewer {
        sql.push_str(
            r#" AND (
                asset.visibility = 'timeline'
                OR (asset."ownerId" = $1 AND asset.visibility = 'archive')
            )"#,
        );
    } else {
        sql.push_str(r#" AND asset.visibility = 'timeline'"#);
    }

    let mut bind_index = if include_viewer { 2 } else { 1 };
    if search.is_favorite.is_some() {
        sql.push_str(&format!(r#" AND asset."isFavorite" = ${bind_index}"#));
        bind_index += 1;
    }
    if search.file_created_after.is_some() {
        sql.push_str(&format!(r#" AND asset."fileCreatedAt" >= ${bind_index}"#));
        bind_index += 1;
    }
    if search.file_created_before.is_some() {
        sql.push_str(&format!(r#" AND asset."fileCreatedAt" <= ${bind_index}"#));
        bind_index += 1;
    }

    sql.push_str(&format!(
        r#"
            AND (
                asset."ownerId" = ANY(${owner})
                OR EXISTS (
                    SELECT 1 FROM album_asset aa
                    WHERE aa."assetId" = asset.id
                      AND aa."albumId" = ANY(${album})
                )
            )
            ORDER BY asset."fileCreatedAt" DESC
        "#,
        owner = bind_index,
        album = bind_index + 1,
    ));

    (sql, include_viewer)
}

pub async fn get_map_markers(
    pool: &Pool<Postgres>,
    search: &MapMarkerSearch,
) -> Result<Vec<MapMarkerRow>, sqlx::Error> {
    if search.owner_ids.is_empty() && search.album_ids.is_empty() {
        return Ok(vec![]);
    }

    let (sql, include_viewer) = map_marker_sql(search);
    let mut query = sqlx::query_as::<_, MapMarkerRow>(&sql);
    if include_viewer {
        query = query.bind(search.auth_user_id);
    }
    if let Some(is_favorite) = search.is_favorite {
        query = query.bind(is_favorite);
    }
    if let Some(after) = search.file_created_after {
        query = query.bind(after);
    }
    if let Some(before) = search.file_created_before {
        query = query.bind(before);
    }
    query = query.bind(&search.owner_ids).bind(&search.album_ids);
    query.fetch_all(pool).await
}

pub async fn reverse_geocode_places(
    pool: &Pool<Postgres>,
    latitude: f64,
    longitude: f64,
) -> Result<Option<ReverseGeocodeRow>, sqlx::Error> {
    sqlx::query_as::<_, ReverseGeocodeRow>(
        r#"
            SELECT
                name as city,
                "admin1Name" as state,
                "countryCode" as country_code
            FROM geodata_places
            WHERE earth_box(
                ll_to_earth_public($1, $2),
                25000
            ) @> ll_to_earth_public(latitude, longitude)
            ORDER BY earth_distance(
                ll_to_earth_public($1, $2),
                ll_to_earth_public(latitude, longitude)
            )
            LIMIT 1
        "#,
    )
    .bind(latitude)
    .bind(longitude)
    .fetch_optional(pool)
    .await
}

pub async fn reverse_geocode_country(
    pool: &Pool<Postgres>,
    latitude: f64,
    longitude: f64,
) -> Result<Option<NaturalEarthRow>, sqlx::Error> {
    sqlx::query_as::<_, NaturalEarthRow>(
        r#"
            SELECT admin, admin_a3
            FROM naturalearth_countries
            WHERE coordinates @> point($2, $1)
            LIMIT 1
        "#,
    )
    .bind(latitude)
    .bind(longitude)
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(archived: Option<bool>) -> MapMarkerSearch {
        MapMarkerSearch {
            owner_ids: vec![Uuid::nil()],
            album_ids: vec![],
            auth_user_id: Uuid::nil(),
            is_archived: archived,
            is_favorite: None,
            file_created_after: None,
            file_created_before: None,
        }
    }

    #[test]
    fn default_map_query_starts_at_first_parameter() {
        let (sql, include_viewer) = map_marker_sql(&search(None));
        assert!(!include_viewer);
        assert!(sql.contains(r#"asset."ownerId" = ANY($1)"#));
        assert!(sql.contains(r#"aa."albumId" = ANY($2)"#));
        assert!(!sql.contains("$3"));
    }

    #[test]
    fn archived_map_query_binds_viewer_before_owners() {
        let (sql, include_viewer) = map_marker_sql(&search(Some(true)));
        assert!(include_viewer);
        assert!(sql.contains(r#"asset."ownerId" = $1 AND"#));
        assert!(sql.contains(r#"asset."ownerId" = ANY($2)"#));
        assert!(sql.contains(r#"aa."albumId" = ANY($3)"#));
    }
}
