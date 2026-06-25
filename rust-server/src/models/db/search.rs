use chrono::{DateTime, Utc};
use sqlx::{Pool, Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    pub user_ids: Vec<Uuid>,
    pub visibility: Option<String>,
    pub library_id: Option<Uuid>,
    pub asset_id: Option<Uuid>,
    pub asset_type: Option<String>,
    pub checksum: Option<Vec<u8>>,
    pub is_favorite: Option<bool>,
    pub is_motion: Option<bool>,
    pub is_offline: Option<bool>,
    pub is_encoded: Option<bool>,
    pub is_not_in_album: Option<bool>,
    pub with_deleted: bool,
    pub with_stacked: Option<bool>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
    pub updated_before: Option<DateTime<Utc>>,
    pub updated_after: Option<DateTime<Utc>>,
    pub trashed_before: Option<DateTime<Utc>>,
    pub trashed_after: Option<DateTime<Utc>>,
    pub taken_before: Option<DateTime<Utc>>,
    pub taken_after: Option<DateTime<Utc>>,
    pub city: Option<Option<String>>,
    pub state: Option<Option<String>>,
    pub country: Option<Option<String>>,
    pub make: Option<Option<String>>,
    pub model: Option<Option<String>>,
    pub lens_model: Option<Option<String>>,
    pub rating: Option<Option<i32>>,
    pub description: Option<String>,
    pub original_file_name: Option<String>,
    pub original_path: Option<String>,
    pub encoded_video_path: Option<String>,
    pub ocr: Option<String>,
    pub person_ids: Option<Vec<Uuid>>,
    pub tag_ids: Option<Option<Vec<Uuid>>>,
    pub album_ids: Option<Vec<Uuid>>,
    pub min_file_size: Option<i64>,
}

pub struct SearchPage {
    pub page: i64,
    pub size: i64,
}

pub async fn search_metadata_ids(
    pool: &Pool<Postgres>,
    filter: &SearchFilter,
    page: &SearchPage,
    order_desc: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let offset = (page.page - 1) * page.size;
    let limit = page.size + 1;

    let mut query = QueryBuilder::new(r#"SELECT asset.id FROM asset "#);
    append_search_from(&mut query, filter);
    query.push(" WHERE 1=1 ");
    append_search_filters(&mut query, filter);
    query.push(" ORDER BY asset.\"fileCreatedAt\" ");
    query.push(if order_desc { "DESC" } else { "ASC" });
    query.push(" LIMIT ");
    query.push_bind(limit);
    query.push(" OFFSET ");
    query.push_bind(offset);

    query.build_query_scalar().fetch_all(pool).await
}

pub async fn search_statistics_count(
    pool: &Pool<Postgres>,
    filter: &SearchFilter,
) -> Result<i64, sqlx::Error> {
    let mut query = QueryBuilder::new(r#"SELECT COUNT(*)::bigint FROM asset "#);
    append_search_from(&mut query, filter);
    query.push(" WHERE 1=1 ");
    append_search_filters(&mut query, filter);
    query.build_query_scalar().fetch_one(pool).await
}

pub async fn search_random_ids(
    pool: &Pool<Postgres>,
    filter: &SearchFilter,
    size: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut query = QueryBuilder::new(r#"SELECT asset.id FROM asset "#);
    append_search_from(&mut query, filter);
    query.push(" WHERE 1=1 ");
    append_search_filters(&mut query, filter);
    query.push(" ORDER BY random() LIMIT ");
    query.push_bind(size);
    query.build_query_scalar().fetch_all(pool).await
}

pub async fn search_large_asset_ids(
    pool: &Pool<Postgres>,
    filter: &SearchFilter,
    size: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut query = QueryBuilder::new(
        r#"
        SELECT asset.id FROM asset
        INNER JOIN asset_exif ON asset.id = asset_exif."assetId"
        "#,
    );
    query.push(" WHERE 1=1 ");
    append_search_filters(&mut query, filter);
    if filter.min_file_size.unwrap_or(0) > 0 {
        query.push(r#" AND asset_exif."fileSizeInByte" > "#);
        query.push_bind(filter.min_file_size.unwrap_or(0));
    } else {
        query.push(r#" AND asset_exif."fileSizeInByte" > 0"#);
    }
    query.push(r#" ORDER BY asset_exif."fileSizeInByte" DESC LIMIT "#);
    query.push_bind(size);
    query.build_query_scalar().fetch_all(pool).await
}

pub async fn search_smart_ids(
    pool: &Pool<Postgres>,
    filter: &SearchFilter,
    embedding: &str,
    page: &SearchPage,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let offset = (page.page - 1) * page.size;
    let limit = page.size + 1;
    let embedding = crate::models::db::smart_search::normalize_embedding(embedding);

    let mut tx = pool.begin().await?;
    // VectorChord probe setting; ignore on plain pgvector installs.
    let _ = sqlx::query("SET LOCAL vchordrq.probes = 1")
        .execute(&mut *tx)
        .await;

    let mut query = QueryBuilder::new(r#"SELECT asset.id FROM asset "#);
    append_search_from(&mut query, filter);
    query.push(r#" INNER JOIN smart_search ON asset.id = smart_search."assetId" "#);
    query.push(" WHERE 1=1 ");
    append_search_filters(&mut query, filter);
    query.push(" ORDER BY smart_search.embedding <=> CAST(");
    query.push_bind(embedding);
    query.push(" AS vector) LIMIT ");
    query.push_bind(limit);
    query.push(" OFFSET ");
    query.push_bind(offset);

    let ids = query.build_query_scalar().fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(ids)
}

pub async fn get_smart_search_embedding(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<String>, sqlx::Error> {
    crate::models::db::smart_search::get_embedding(pool, asset_id).await
}

pub async fn search_places(
    pool: &Pool<Postgres>,
    name: &str,
) -> Result<Vec<PlaceRow>, sqlx::Error> {
    sqlx::query_as::<_, PlaceRow>(
        r#"
            SELECT
                name,
                latitude,
                longitude,
                "admin1Name" as admin1_name,
                "admin2Name" as admin2_name
            FROM geodata_places
            WHERE f_unaccent(name) %>> f_unaccent($1)
               OR f_unaccent("admin2Name") %>> f_unaccent($1)
               OR f_unaccent("admin1Name") %>> f_unaccent($1)
               OR f_unaccent("alternateNames") %>> f_unaccent($1)
            ORDER BY
                coalesce(f_unaccent(name) <->>> f_unaccent($1), 0.1) +
                coalesce(f_unaccent("admin2Name") <->>> f_unaccent($1), 0.1) +
                coalesce(f_unaccent("admin1Name") <->>> f_unaccent($1), 0.1) +
                coalesce(f_unaccent("alternateNames") <->>> f_unaccent($1), 0.1)
            LIMIT 20
        "#,
    )
    .bind(name)
    .fetch_all(pool)
    .await
}

pub async fn get_assets_by_city_ids(
    pool: &Pool<Postgres>,
    user_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if user_ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_scalar(
        r#"
            WITH RECURSIVE cte AS (
                SELECT ae.city, ae."assetId" AS asset_id
                FROM asset_exif ae
                INNER JOIN asset ON asset.id = ae."assetId"
                WHERE asset."ownerId" = ANY($1)
                  AND asset.visibility = 'timeline'
                  AND asset.type = 'IMAGE'
                  AND asset."deletedAt" IS NULL
                  AND ae.city IS NOT NULL
                ORDER BY ae.city
                LIMIT 1
                UNION ALL
                SELECT l.city, l.asset_id
                FROM cte
                INNER JOIN LATERAL (
                    SELECT ae.city, ae."assetId" AS asset_id
                    FROM asset_exif ae
                    INNER JOIN asset ON asset.id = ae."assetId"
                    WHERE asset."ownerId" = ANY($1)
                      AND asset.visibility = 'timeline'
                      AND asset.type = 'IMAGE'
                      AND asset."deletedAt" IS NULL
                      AND ae.city IS NOT NULL
                      AND ae.city > cte.city
                    ORDER BY ae.city
                    LIMIT 1
                ) l ON TRUE
            )
            SELECT asset.id
            FROM asset
            INNER JOIN asset_exif ON asset.id = asset_exif."assetId"
            INNER JOIN cte ON asset.id = cte.asset_id
            ORDER BY asset_exif.city
        "#,
    )
    .bind(user_ids)
    .fetch_all(pool)
    .await
}

#[derive(Debug, sqlx::FromRow)]
struct ExploreCityRow {
    id: Uuid,
    city: String,
}

#[derive(Debug, sqlx::FromRow)]
struct ExploreRecentRow {
    id: Uuid,
    created_at: DateTime<Utc>,
}

pub async fn get_explore_city_asset_ids(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    min_assets_per_field: i64,
    max_fields: i64,
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    let rows = sqlx::query_as::<_, ExploreCityRow>(
        r#"
            WITH cities AS (
                SELECT city
                FROM asset_exif
                WHERE city IS NOT NULL
                GROUP BY city
                HAVING COUNT("assetId") >= $1
            )
            SELECT DISTINCT ON (asset_exif.city)
                asset.id,
                asset_exif.city
            FROM asset
            INNER JOIN asset_exif ON asset.id = asset_exif."assetId"
            INNER JOIN cities ON asset_exif.city = cities.city
            WHERE asset."ownerId" = $2
              AND asset.visibility = 'timeline'
              AND asset.type = 'IMAGE'
              AND asset."deletedAt" IS NULL
            LIMIT $3
        "#,
    )
    .bind(min_assets_per_field)
    .bind(owner_id)
    .bind(max_fields)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|row| (row.id, row.city)).collect())
}

pub async fn get_explore_recent_asset_ids(
    pool: &Pool<Postgres>,
    owner_id: &Uuid,
    max_assets: i64,
) -> Result<Vec<(Uuid, DateTime<Utc>)>, sqlx::Error> {
    let rows = sqlx::query_as::<_, ExploreRecentRow>(
        r#"
            SELECT id, "createdAt" as created_at
            FROM asset
            WHERE "ownerId" = $1
              AND visibility = 'timeline'
              AND type = 'IMAGE'
              AND "deletedAt" IS NULL
            ORDER BY "createdAt" DESC
            LIMIT $2
        "#,
    )
    .bind(owner_id)
    .bind(max_assets)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|row| (row.id, row.created_at)).collect())
}

pub async fn get_exif_suggestions(
    pool: &Pool<Postgres>,
    field: &str,
    user_ids: &[Uuid],
    country: Option<&str>,
    state: Option<&str>,
    make: Option<&str>,
    model: Option<&str>,
) -> Result<Vec<String>, sqlx::Error> {
    if user_ids.is_empty() {
        return Ok(vec![]);
    }

    let allowed = ["country", "state", "city", "make", "model", "lensModel"];
    if !allowed.contains(&field) {
        return Ok(vec![]);
    }

    let mut query = QueryBuilder::new(format!(
        r#"
        SELECT DISTINCT asset_exif."{field}"
        FROM asset_exif
        INNER JOIN asset ON asset.id = asset_exif."assetId"
        WHERE asset."ownerId" = ANY(
        "#
    ));
    query.push_bind(user_ids);
    query.push(
        r#")
          AND asset.visibility = 'timeline'
          AND asset."deletedAt" IS NULL
          AND asset_exif."#,
    );
    query.push(field);
    query.push(r#" IS NOT NULL AND asset_exif."#);
    query.push(field);
    query.push(" != '' ");

    if let Some(country) = country {
        query.push(r#" AND asset_exif.country = "#);
        query.push_bind(country);
    }
    if let Some(state) = state {
        query.push(r#" AND asset_exif.state = "#);
        query.push_bind(state);
    }
    if let Some(make) = make {
        query.push(r#" AND asset_exif.make = "#);
        query.push_bind(make);
    }
    if let Some(model) = model {
        query.push(r#" AND asset_exif.model = "#);
        query.push_bind(model);
    }

    query.push(" ORDER BY 1");
    query.build_query_scalar().fetch_all(pool).await
}

#[derive(Debug, sqlx::FromRow)]
pub struct PlaceRow {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub admin1_name: Option<String>,
    pub admin2_name: Option<String>,
}

fn append_search_from(query: &mut QueryBuilder<'_, Postgres>, filter: &SearchFilter) {
    if needs_exif_join(filter) || filter.ocr.is_some() {
        query.push(r#" INNER JOIN asset_exif ON asset.id = asset_exif."assetId" "#);
    }
    if filter.ocr.is_some() {
        query.push(r#" INNER JOIN ocr_search ON asset.id = ocr_search."assetId" "#);
    }
}

fn needs_exif_join(filter: &SearchFilter) -> bool {
    filter.city.is_some()
        || filter.state.is_some()
        || filter.country.is_some()
        || filter.make.is_some()
        || filter.model.is_some()
        || filter.lens_model.is_some()
        || filter.rating.is_some()
        || filter.description.is_some()
        || filter.min_file_size.is_some()
}

fn append_search_filters(query: &mut QueryBuilder<'_, Postgres>, filter: &SearchFilter) {
    let visibility = filter
        .visibility
        .clone()
        .unwrap_or_else(|| "timeline".to_string());
    query.push(r#" AND asset.visibility = "#);
    query.push_bind(visibility);
    query.push("::asset_visibility_enum");

    if !filter.user_ids.is_empty() {
        query.push(r#" AND asset."ownerId" = ANY("#);
        query.push_bind(filter.user_ids.clone());
        query.push("::uuid[]) ");
    }

    if let Some(album_ids) = &filter.album_ids {
        if !album_ids.is_empty() {
            query.push(
                r#"
                AND EXISTS (
                    SELECT 1 FROM album_asset
                    WHERE album_asset."assetId" = asset.id
                      AND album_asset."albumId" = ANY(
                "#,
            );
            query.push_bind(album_ids.clone());
            query.push(
                r#")
                    GROUP BY album_asset."assetId"
                    HAVING COUNT(DISTINCT album_asset."albumId") = "#,
            );
            query.push_bind(album_ids.len() as i64);
            query.push(") ");
        }
    }

    if let Some(tag_ids) = &filter.tag_ids {
        match tag_ids {
            None => {
                query.push(
                    r#" AND NOT EXISTS (SELECT 1 FROM tag_asset WHERE tag_asset."assetId" = asset.id) "#,
                );
            }
            Some(ids) if !ids.is_empty() => {
                query.push(
                    r#"
                    AND EXISTS (
                        SELECT 1 FROM tag_asset
                        WHERE tag_asset."assetId" = asset.id
                          AND tag_asset."tagId" = ANY(
                    "#,
                );
                query.push_bind(ids.clone());
                query.push(
                    r#")
                        GROUP BY tag_asset."assetId"
                        HAVING COUNT(DISTINCT tag_asset."tagId") = "#,
                );
                query.push_bind(ids.len() as i64);
                query.push(") ");
            }
            _ => {}
        }
    }

    if let Some(person_ids) = &filter.person_ids {
        if !person_ids.is_empty() {
            query.push(
                r#"
                AND EXISTS (
                    SELECT 1 FROM asset_face
                    WHERE asset_face."assetId" = asset.id
                      AND asset_face."personId" = ANY(
                "#,
            );
            query.push_bind(person_ids.clone());
            query.push(
                r#")
                      AND asset_face."deletedAt" IS NULL
                      AND asset_face."isVisible" = TRUE
                    GROUP BY asset_face."assetId"
                    HAVING COUNT(DISTINCT asset_face."personId") = "#,
            );
            query.push_bind(person_ids.len() as i64);
            query.push(") ");
        }
    }

    append_date_filter(query, r#"asset."createdAt""#, filter.created_before, filter.created_after);
    append_date_filter(query, r#"asset."updatedAt""#, filter.updated_before, filter.updated_after);
    append_date_filter(query, r#"asset."deletedAt""#, filter.trashed_before, filter.trashed_after);
    append_date_filter(
        query,
        r#"asset."fileCreatedAt""#,
        filter.taken_before,
        filter.taken_after,
    );

    append_nullable_exif_string(query, "city", filter.city.as_ref());
    append_nullable_exif_string(query, "state", filter.state.as_ref());
    append_nullable_exif_string(query, "country", filter.country.as_ref());
    append_nullable_exif_string(query, "make", filter.make.as_ref());
    append_nullable_exif_string(query, "model", filter.model.as_ref());
    append_nullable_exif_string(query, "lensModel", filter.lens_model.as_ref());

    if let Some(rating) = &filter.rating {
        if needs_exif_join(filter) {
            query.push(r#" AND asset_exif.rating "#);
            if rating.is_none() {
                query.push("IS NULL ");
            } else {
                query.push("= ");
                query.push_bind(rating.unwrap());
            }
        } else {
            query.push(
                r#" AND EXISTS (
                    SELECT 1 FROM asset_exif ae
                    WHERE ae."assetId" = asset.id
                      AND ae.rating "#,
            );
            if rating.is_none() {
                query.push("IS NULL) ");
            } else {
                query.push("= ");
                query.push_bind(rating.unwrap());
                query.push(") ");
            }
        }
    }

    if let Some(checksum) = &filter.checksum {
        query.push(r#" AND asset.checksum = "#);
        query.push_bind(checksum.clone());
    }
    if let Some(id) = filter.asset_id {
        query.push(r#" AND asset.id = "#);
        query.push_bind(id);
    }
    if let Some(library_id) = filter.library_id {
        query.push(r#" AND asset."libraryId" = "#);
        query.push_bind(library_id);
    }

    if let Some(path) = &filter.original_path {
        query.push(r#" AND f_unaccent(asset."originalPath") ILIKE '%' || f_unaccent("#);
        query.push_bind(path.clone());
        query.push(") || '%' ");
    }
    if let Some(name) = &filter.original_file_name {
        query.push(r#" AND f_unaccent(asset."originalFileName") ILIKE '%' || f_unaccent("#);
        query.push_bind(name.clone());
        query.push(") || '%' ");
    }
    if let Some(description) = &filter.description {
        if needs_exif_join(filter) {
            query.push(r#" AND f_unaccent(asset_exif.description) ILIKE '%' || f_unaccent("#);
            query.push_bind(description.clone());
            query.push(") || '%' ");
        } else {
            query.push(
                r#" AND EXISTS (
                    SELECT 1 FROM asset_exif ae
                    WHERE ae."assetId" = asset.id
                      AND f_unaccent(ae.description) ILIKE '%' || f_unaccent("#,
            );
            query.push_bind(description.clone());
            query.push(") || '%') ");
        }
    }

    if let Some(ocr) = &filter.ocr {
        let tokens = crate::utils::search::tokenize_for_search(ocr).join(" ");
        query.push(r#" AND f_unaccent(ocr_search.text) %>> f_unaccent("#);
        query.push_bind(tokens);
        query.push(") ");
    }

    if let Some(asset_type) = &filter.asset_type {
        query.push(r#" AND asset.type = "#);
        query.push_bind(asset_type.clone());
    }
    if let Some(is_favorite) = filter.is_favorite {
        query.push(r#" AND asset."isFavorite" = "#);
        query.push_bind(is_favorite);
    }
    if let Some(is_offline) = filter.is_offline {
        query.push(r#" AND asset."isOffline" = "#);
        query.push_bind(is_offline);
    }
    if let Some(is_motion) = filter.is_motion {
        if is_motion {
            query.push(r#" AND asset."livePhotoVideoId" IS NOT NULL "#);
        } else {
            query.push(r#" AND asset."livePhotoVideoId" IS NULL "#);
        }
    }
    if let Some(is_encoded) = filter.is_encoded {
        let exists = r#"
            EXISTS (
                SELECT 1 FROM asset_file
                WHERE asset_file."assetId" = asset.id
                  AND asset_file.type = 'encoded_video'
            )
        "#;
        if is_encoded {
            query.push(" AND ");
            query.push(exists);
        } else {
            query.push(" AND NOT ");
            query.push(exists);
        }
    }

    if filter.is_not_in_album == Some(true)
        && filter.album_ids.as_ref().is_none_or(|ids| ids.is_empty())
    {
        query.push(
            r#" AND NOT EXISTS (SELECT 1 FROM album_asset WHERE album_asset."assetId" = asset.id) "#,
        );
    }

    if filter.with_stacked == Some(false) {
        query.push(r#" AND asset."stackId" IS NULL "#);
    }

    if let Some(path) = &filter.encoded_video_path {
        query.push(
            r#"
            AND EXISTS (
                SELECT 1 FROM asset_file
                WHERE asset_file."assetId" = asset.id
                  AND asset_file.type = 'encoded_video'
                  AND asset_file."isEdited" = FALSE
                  AND asset_file.path =
            "#,
        );
        query.push_bind(path.clone());
        query.push(") ");
    }

    if !filter.with_deleted {
        query.push(r#" AND asset."deletedAt" IS NULL "#);
    }
}

fn append_date_filter(
    query: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    before: Option<DateTime<Utc>>,
    after: Option<DateTime<Utc>>,
) {
    if let Some(before) = before {
        query.push(format!(" AND {column} <= "));
        query.push_bind(before);
    }
    if let Some(after) = after {
        query.push(format!(" AND {column} >= "));
        query.push_bind(after);
    }
}

fn append_nullable_exif_string(
    query: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    value: Option<&Option<String>>,
) {
    let Some(value) = value else { return };
    query.push(format!(r#" AND asset_exif."{column}" "#));
    match value {
        None => {
            query.push("IS NULL ");
        }
        Some(text) => {
            query.push("= ");
            query.push_bind(text.clone());
        }
    }
}
