use serde::Serialize;
use sqlx::{FromRow, Pool, Postgres, Row};
use uuid::Uuid;

use super::person_schema::PersonSchema;

#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

#[derive(Debug, Clone, Default)]
pub struct TimelineFilter {
    pub owner_ids: Vec<Uuid>,
    pub album_id: Option<Uuid>,
    pub tag_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub bbox: Option<BoundingBox>,
    pub is_favorite: Option<bool>,
    pub is_trashed: bool,
    pub visibility: Option<String>,
    pub use_default_visibility: bool,
    pub with_stacked: bool,
    pub shared_link_id: Option<Uuid>,
    pub order_by_taken_at: bool,
    pub order_desc: bool,
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TimeBucketItem {
    #[sqlx(rename = "timeBucket")]
    pub time_bucket: String,
    pub count: i64,
}

pub async fn get_time_buckets(
    pool: &Pool<Postgres>,
    filter: &TimelineFilter,
) -> Result<Vec<TimeBucketItem>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let date_expr = if filter.order_by_taken_at {
        r#"date_trunc('MONTH', asset."localDateTime" AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'"#
    } else {
        r#"date_trunc('MONTH', asset."createdAt" AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'"#
    };

    let mut query = sqlx::QueryBuilder::new(format!(
        r#"
        WITH asset AS (
            SELECT {date_expr} AS "timeBucket"
            FROM asset
        "#
    ));

    query.push(" WHERE 1=1 ");
    append_asset_filters(&mut query, filter, true, &schema);

    query.push(
        r#"
        )
        SELECT ("timeBucket" AT TIME ZONE 'UTC')::date::text AS "timeBucket",
               COUNT(*)::bigint AS count
        FROM asset
        GROUP BY "timeBucket"
        "#,
    );

    if filter.order_desc {
        query.push(r#"ORDER BY "timeBucket" DESC"#);
    } else {
        query.push(r#"ORDER BY "timeBucket" ASC"#);
    }

    query.build_query_as::<TimeBucketItem>().fetch_all(pool).await
}

pub async fn get_time_bucket_json(
    pool: &Pool<Postgres>,
    auth_user_id: Uuid,
    time_bucket: &str,
    filter: &TimelineFilter,
    include_exif: bool,
    include_coordinates: bool,
) -> Result<String, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let date_expr = if filter.order_by_taken_at {
        r#"date_trunc('MONTH', asset."localDateTime" AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'"#
    } else {
        r#"date_trunc('MONTH', asset."createdAt" AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'"#
    };

    let order_date_expr = if filter.order_by_taken_at {
        r#"(asset."localDateTime" AT TIME ZONE 'UTC')::date"#
    } else {
        r#"(asset."createdAt" AT TIME ZONE 'UTC')::date"#
    };

    let order_dir = if filter.order_desc { "DESC" } else { "ASC" };

    let mut query = sqlx::QueryBuilder::new(format!(
        r#"
        WITH cte AS (
            SELECT
                asset.duration,
                asset.id,
                asset.visibility,
                asset."isFavorite" AND asset."ownerId" = "#,
    ));

    query.push_bind(auth_user_id);
    query.push(
        r#" AS "isFavorite",
                asset.type = 'IMAGE' AS "isImage",
                asset."deletedAt" IS NOT NULL AS "isTrashed",
                asset."livePhotoVideoId",
                EXTRACT(
                    EPOCH FROM (
                        asset."localDateTime" AT TIME ZONE 'UTC' - asset."fileCreatedAt" AT TIME ZONE 'UTC'
                    )
                )::real / 3600 AS "localOffsetHours",
                asset."ownerId",
                asset.status,
                asset."fileCreatedAt" AT TIME ZONE 'UTC' AS "fileCreatedAt",
                asset."createdAt" AT TIME ZONE 'UTC' AS "createdAt",
                encode(asset.thumbhash, 'base64') AS thumbhash,
                asset_exif."projectionType",
                COALESCE(
                    CASE
                        WHEN asset.height = 0 OR asset.width = 0 THEN 1
                        ELSE ROUND(asset.width::numeric / asset.height::numeric, 3)
                    END,
                    1
                ) AS ratio
        "#,
    );

    if include_exif {
        query.push(
            r#",
                asset_exif.city,
                asset_exif.country
            "#,
        );
    }

    if include_coordinates {
        query.push(
            r#",
                asset_exif.latitude,
                asset_exif.longitude
            "#,
        );
    }

    if filter.with_stacked {
        query.push(
            r#",
                stacked_assets.stack
            "#,
        );
    }

    query.push(
        r#"
            FROM asset
            INNER JOIN asset_exif ON asset.id = asset_exif."assetId"
        "#,
    );

    if filter.with_stacked {
        query.push(
            r#"
            LEFT JOIN LATERAL (
                SELECT array[stacked."stackId"::text, COUNT(stacked)::text] AS stack
                FROM asset AS stacked
                WHERE stacked."stackId" = asset."stackId"
                  AND stacked."deletedAt" IS NULL
                  AND stacked.visibility = 'timeline'
                GROUP BY stacked."stackId"
            ) AS stacked_assets ON TRUE
            "#,
        );
    }

    query.push(" WHERE 1=1 ");
    append_asset_filters(&mut query, filter, true, &schema);

    query.push(format!(r#" AND {date_expr} = "#));
    query.push_bind(parse_time_bucket(time_bucket));
    query.push("::timestamptz ");

    if filter.with_stacked {
        query.push(
            r#"
            AND NOT EXISTS (
                SELECT 1 FROM stack
                WHERE stack.id = asset."stackId"
                  AND stack."primaryAssetId" != asset.id
            )
            "#,
        );
    }

    query.push(
        r#"
            ORDER BY 
        "#,
    );
    query.push(order_date_expr);
    query.push(" ");
    query.push(order_dir);
    query.push(
        r#", asset."fileCreatedAt" "#,
    );
    query.push(order_dir);
    query.push(
        r#", asset."originalFileName" "#,
    );
    query.push(order_dir);
    query.push(
        r#"
        ),
        agg AS (
            SELECT
                COALESCE(array_agg(duration), '{}') AS duration,
                COALESCE(array_agg(id), '{}') AS id,
                COALESCE(array_agg(visibility), '{}') AS visibility,
                COALESCE(array_agg("isFavorite"), '{}') AS "isFavorite",
                COALESCE(array_agg("isImage"), '{}') AS "isImage",
                COALESCE(array_agg("isTrashed"), '{}') AS "isTrashed",
                COALESCE(array_agg("livePhotoVideoId"), '{}') AS "livePhotoVideoId",
                COALESCE(array_agg("fileCreatedAt"), '{}') AS "fileCreatedAt",
                COALESCE(array_agg("localOffsetHours"), '{}') AS "localOffsetHours",
                COALESCE(array_agg("createdAt"), '{}') AS "createdAt",
                COALESCE(array_agg("ownerId"), '{}') AS "ownerId",
                COALESCE(array_agg("projectionType"), '{}') AS "projectionType",
                COALESCE(array_agg(ratio), '{}') AS ratio,
                COALESCE(array_agg(status), '{}') AS status,
                COALESCE(array_agg(thumbhash), '{}') AS thumbhash
        "#,
    );

    if include_exif {
        query.push(
            r#",
                COALESCE(array_agg(city), '{}') AS city,
                COALESCE(array_agg(country), '{}') AS country
            "#,
        );
    }

    if include_coordinates {
        query.push(
            r#",
                COALESCE(array_agg(latitude), '{}') AS latitude,
                COALESCE(array_agg(longitude), '{}') AS longitude
            "#,
        );
    }

    if filter.with_stacked {
        query.push(
            r#",
                COALESCE(json_agg(stack), '[]') AS stack
            "#,
        );
    }

    query.push(
        r#"
            FROM cte
        )
        SELECT COALESCE(to_json(agg)::text, '{}') AS assets
        FROM agg
        "#,
    );

    let row = query.build().fetch_one(pool).await?;
    Ok(row.get("assets"))
}

fn parse_time_bucket(value: &str) -> String {
    value.trim_start_matches(['+', '-']).to_string()
}

fn append_asset_filters(
    query: &mut sqlx::QueryBuilder<'_, Postgres>,
    filter: &TimelineFilter,
    for_bucket: bool,
    schema: &PersonSchema,
) {
    if filter.is_trashed {
        query.push(r#" AND asset."deletedAt" IS NOT NULL "#);
        query.push(r#" AND asset.status != 'deleted' "#);
    } else {
        query.push(r#" AND asset."deletedAt" IS NULL "#);
    }

    if let Some(visibility) = &filter.visibility {
        crate::utils::query::push_visibility_enum_eq(
            query,
            "AND asset.visibility",
            visibility.clone(),
        );
    } else if filter.use_default_visibility {
        query.push(r#" AND asset.visibility IN ('archive', 'timeline') "#);
    }

    if !filter.owner_ids.is_empty() {
        query.push(r#" AND asset."ownerId" = ANY("#);
        query.push_bind(filter.owner_ids.clone());
        query.push("::uuid[]) ");
    }

    if let Some(album_id) = filter.album_id {
        if for_bucket {
            query.push(
                r#"
                AND EXISTS (
                    SELECT 1 FROM album_asset
                    WHERE album_asset."assetId" = asset.id
                      AND album_asset."albumId" =
                "#,
            );
        } else {
            query.push(r#" INNER JOIN album_asset ON album_asset."assetId" = asset.id AND album_asset."albumId" = "#);
        }
        query.push_bind(album_id);
        if for_bucket {
            query.push(") ");
        }
    }

    if let Some(tag_id) = filter.tag_id {
        query.push(
            r#"
            AND EXISTS (
                SELECT 1
                FROM tag_closure
                INNER JOIN tag_asset ON tag_asset."tagId" = tag_closure.id_descendant
                WHERE tag_asset."assetId" = asset.id
                  AND tag_closure.id_ancestor =
            "#,
        );
        query.push_bind(tag_id);
        query.push(") ");
    }

    if let Some(is_favorite) = filter.is_favorite {
        query.push(r#" AND asset."isFavorite" = "#);
        query.push_bind(is_favorite);
    }

    if let Some(person_id) = filter.person_id {
        let face_col = schema.face_person_col_quoted();
        query.push(
            format!(
                r#"
            AND EXISTS (
                SELECT 1 FROM asset_face
                WHERE asset_face."assetId" = asset.id
                  AND asset_face.{face_col} =
            "#
            ),
        );
        query.push_bind(person_id);
        query.push(
            r#"
                  AND asset_face."deletedAt" IS NULL
                  AND asset_face."isVisible" = TRUE
            )
            "#,
        );
    }

    if let Some(bbox) = filter.bbox {
        append_bbox_filter(query, bbox);
    }

    if let Some(shared_link_id) = filter.shared_link_id {
        query.push(
            r#"
            AND EXISTS (
                SELECT 1 FROM shared_link_asset
                WHERE shared_link_asset."sharedLinkId" =
            "#,
        );
        query.push_bind(shared_link_id);
        query.push(
            r#" AND shared_link_asset."assetId" = asset.id
            ) "#,
        );
    }

    if filter.with_stacked {
        query.push(
            r#"
            AND (
                asset."stackId" IS NULL
                OR EXISTS (
                    SELECT 1 FROM stack
                    WHERE stack.id = asset."stackId"
                      AND stack."primaryAssetId" = asset.id
                )
            )
            "#,
        );
    }
}

fn append_bbox_filter(query: &mut sqlx::QueryBuilder<'_, Postgres>, bbox: BoundingBox) {
    let east_unwrapped = if bbox.west <= bbox.east {
        bbox.east
    } else {
        bbox.east + 360.0
    };
    let center_longitude = (((bbox.west + east_unwrapped) / 2.0 + 540.0) % 360.0) - 180.0;
    let center_latitude = (bbox.south + bbox.north) / 2.0;

    query.push(
        r#"
        AND EXISTS (
            SELECT 1 FROM asset_exif ae
            WHERE ae."assetId" = asset.id
              AND ae.latitude >= "#,
    );
    query.push_bind(bbox.south);
    query.push(r#" AND ae.latitude <= "#);
    query.push_bind(bbox.north);
    query.push(
        r#"
              AND earth_box(
                    ll_to_earth_public("#,
    );
    query.push_bind(center_latitude);
    query.push(", ");
    query.push_bind(center_longitude);
    query.push(
        r#"), GREATEST(
                    earth_distance(
                        ll_to_earth_public("#,
    );
    query.push_bind(center_latitude);
    query.push(", ");
    query.push_bind(center_longitude);
    query.push(", ll_to_earth_public(");
    query.push_bind(bbox.south);
    query.push(", ");
    query.push_bind(bbox.west);
    query.push(
        r#")),
                    earth_distance(
                        ll_to_earth_public("#,
    );
    query.push_bind(center_latitude);
    query.push(", ");
    query.push_bind(center_longitude);
    query.push(", ll_to_earth_public(");
    query.push_bind(bbox.south);
    query.push(", ");
    query.push_bind(bbox.east);
    query.push(
        r#")),
                    earth_distance(
                        ll_to_earth_public("#,
    );
    query.push_bind(center_latitude);
    query.push(", ");
    query.push_bind(center_longitude);
    query.push(", ll_to_earth_public(");
    query.push_bind(bbox.north);
    query.push(", ");
    query.push_bind(bbox.west);
    query.push(
        r#")),
                    earth_distance(
                        ll_to_earth_public("#,
    );
    query.push_bind(center_latitude);
    query.push(", ");
    query.push_bind(center_longitude);
    query.push(", ll_to_earth_public(");
    query.push_bind(bbox.north);
    query.push(", ");
    query.push_bind(bbox.east);
    query.push(
        r#"))
                  )) @> ll_to_earth_public(ae.latitude, ae.longitude)
        "#,
    );

    if bbox.west <= bbox.east {
        query.push(r#" AND ae.longitude >= "#);
        query.push_bind(bbox.west);
        query.push(r#" AND ae.longitude <= "#);
        query.push_bind(bbox.east);
    } else {
        query.push(r#" AND (ae.longitude >= "#);
        query.push_bind(bbox.west);
        query.push(r#" OR ae.longitude <= "#);
        query.push_bind(bbox.east);
        query.push(") ");
    }

    query.push(") ");
}

pub async fn user_has_album_access(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    album_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT EXISTS(
                SELECT 1 FROM album_user
                WHERE "albumId" = $1 AND "userId" = $2
            )
        "#,
    )
    .bind(album_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn user_owns_tag(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    tag_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM tag WHERE id = $1 AND "userId" = $2)"#,
    )
    .bind(tag_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn user_owns_person(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
    person_id: &Uuid,
) -> Result<bool, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    sqlx::query_scalar(&format!(
        r#"SELECT EXISTS(SELECT 1 FROM person WHERE {where_id} AND "ownerId" = $2)"#,
        where_id = schema.where_person_id("", "$1"),
    ))
    .bind(person_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn get_timeline_partner_ids(
    pool: &Pool<Postgres>,
    user_id: &Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
            SELECT "sharedById"
            FROM partner
            WHERE "sharedWithId" = $1 AND "inTimeline" = TRUE
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}
