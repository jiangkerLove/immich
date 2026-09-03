use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::utils::bytes::hex_or_buffer_to_base64;
use crate::utils::sync::SyncAck;
use crate::models::db::person_schema::PersonSchema;

#[derive(Debug, Clone)]
pub struct SyncQueryOptions {
    pub now_id: String,
    pub user_id: Uuid,
    pub ack: Option<SyncAck>,
}

#[derive(Debug, Clone)]
pub struct SyncBackfillOptions {
    pub now_id: String,
    pub after_update_id: Option<String>,
    pub before_update_id: String,
}

#[derive(Debug, Clone)]
pub struct SyncCreatedAfterOptions {
    pub now_id: String,
    pub user_id: Uuid,
    pub after_create_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SyncDelete {
    pub audit_id: String,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub struct SyncUpsert {
    pub update_id: String,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub struct SyncCreatedAfter {
    pub id: Uuid,
    pub create_id: String,
}

#[derive(Debug, Clone)]
pub struct SyncPartnerCreatedAfter {
    pub shared_by_id: Uuid,
    pub create_id: String,
}

#[derive(Debug, Clone)]
pub struct AlbumUserRow {
    pub user_id: Uuid,
    pub role: String,
}

fn dt(v: DateTime<Utc>) -> Value {
    Value::String(v.to_rfc3339())
}

fn opt_date(v: Option<NaiveDate>) -> Value {
    match v {
        Some(v) => Value::String(v.to_string()),
        None => Value::Null,
    }
}

fn opt_dt(v: Option<DateTime<Utc>>) -> Value {
    match v {
        Some(v) => dt(v),
        None => Value::Null,
    }
}

fn map_asset_fields(
    id: Uuid,
    owner_id: Uuid,
    original_file_name: String,
    thumbhash: Option<Vec<u8>>,
    checksum: Vec<u8>,
    file_created_at: DateTime<Utc>,
    file_modified_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    local_date_time: DateTime<Utc>,
    asset_type: String,
    deleted_at: Option<DateTime<Utc>>,
    visibility: String,
    duration: Option<String>,
    live_photo_video_id: Option<Uuid>,
    stack_id: Option<Uuid>,
    library_id: Option<Uuid>,
    width: Option<i32>,
    height: Option<i32>,
    is_edited: bool,
    is_favorite: bool,
) -> Value {
    json!({
        "id": id,
        "ownerId": owner_id,
        "originalFileName": original_file_name,
        "thumbhash": thumbhash.map(|b| hex_or_buffer_to_base64(&b)),
        "checksum": hex_or_buffer_to_base64(&checksum),
        "fileCreatedAt": dt(file_created_at),
        "fileModifiedAt": dt(file_modified_at),
        "createdAt": dt(created_at),
        "localDateTime": dt(local_date_time),
        "type": asset_type,
        "deletedAt": opt_dt(deleted_at),
        "visibility": visibility,
        "duration": duration,
        "livePhotoVideoId": live_photo_video_id,
        "stackId": stack_id,
        "libraryId": library_id,
        "width": width,
        "height": height,
        "isEdited": is_edited,
        "isFavorite": is_favorite,
    })
}

fn map_asset_exif_upsert(row: &sqlx::postgres::PgRow, update_id_col: &str) -> SyncUpsert {
    SyncUpsert {
        update_id: row.get(update_id_col),
        data: json!({
            "assetId": row.get::<Uuid, _>("assetId"),
            "description": row.get::<Option<String>, _>("description"),
            "exifImageWidth": row.get::<Option<i32>, _>("exifImageWidth"),
            "exifImageHeight": row.get::<Option<i32>, _>("exifImageHeight"),
            "fileSizeInByte": row.get::<Option<i64>, _>("fileSizeInByte"),
            "orientation": row.get::<Option<String>, _>("orientation"),
            "dateTimeOriginal": opt_dt(row.get("dateTimeOriginal")),
            "modifyDate": opt_dt(row.get("modifyDate")),
            "timeZone": row.get::<Option<String>, _>("timeZone"),
            "latitude": row.get::<Option<f64>, _>("latitude"),
            "longitude": row.get::<Option<f64>, _>("longitude"),
            "projectionType": row.get::<Option<String>, _>("projectionType"),
            "city": row.get::<Option<String>, _>("city"),
            "state": row.get::<Option<String>, _>("state"),
            "country": row.get::<Option<String>, _>("country"),
            "make": row.get::<Option<String>, _>("make"),
            "model": row.get::<Option<String>, _>("model"),
            "lensModel": row.get::<Option<String>, _>("lensModel"),
            "fNumber": row.get::<Option<f64>, _>("fNumber"),
            "focalLength": row.get::<Option<f64>, _>("focalLength"),
            "iso": row.get::<Option<i32>, _>("iso"),
            "exposureTime": row.get::<Option<String>, _>("exposureTime"),
            "profileDescription": row.get::<Option<String>, _>("profileDescription"),
            "rating": row.get::<Option<i32>, _>("rating"),
            "fps": row.get::<Option<f64>, _>("fps"),
        }),
    }
}

// ---------------------------------------------------------------------------
// Album
// ---------------------------------------------------------------------------

pub async fn album_get_created_after(
    pool: &PgPool,
    options: &SyncCreatedAfterOptions,
) -> Result<Vec<SyncCreatedAfter>, sqlx::Error> {
    let rows = match &options.after_create_id {
        Some(after) => {
            sqlx::query(
                r#"
                SELECT "albumId" as id, "createId"::text as create_id
                FROM "album_user"
                WHERE "userId" = $1 AND "createId" >= $2::uuid AND "createId" < $3::uuid
                ORDER BY "createId" ASC
                "#,
            )
            .bind(options.user_id)
            .bind(after)
            .bind(&options.now_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "albumId" as id, "createId"::text as create_id
                FROM "album_user"
                WHERE "userId" = $1 AND "createId" < $2::uuid
                ORDER BY "createId" ASC
                "#,
            )
            .bind(options.user_id)
            .bind(&options.now_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncCreatedAfter {
            id: row.get("id"),
            create_id: row.get("create_id"),
        })
        .collect())
}

pub async fn album_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "albumId"
                FROM "album_audit"
                WHERE "album_audit"."id" < $1::uuid
                  AND "album_audit"."id" > $2::uuid
                  AND "userId" = $3
                ORDER BY "album_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "albumId"
                FROM "album_audit"
                WHERE "album_audit"."id" < $1::uuid
                  AND "userId" = $2
                ORDER BY "album_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({ "albumId": row.get::<Uuid, _>("albumId") }),
        })
        .collect())
}

pub async fn album_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT DISTINCT ON ("album"."id", "album"."updateId")
                  "album"."id",
                  "album"."albumName" as name,
                  "album"."description",
                  "album"."createdAt",
                  "album"."updatedAt",
                  "album"."albumThumbnailAssetId" as "thumbnailAssetId",
                  "album"."isActivityEnabled",
                  "album"."order",
                  "album"."updateId"::text as update_id
                FROM "album"
                LEFT JOIN "album_user" as "album_users" ON "album"."id" = "album_users"."albumId"
                WHERE "album"."updateId" < $1::uuid
                  AND "album"."updateId" > $2::uuid
                  AND "album_users"."userId" = $3
                ORDER BY "album"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT DISTINCT ON ("album"."id", "album"."updateId")
                  "album"."id",
                  "album"."albumName" as name,
                  "album"."description",
                  "album"."createdAt",
                  "album"."updatedAt",
                  "album"."albumThumbnailAssetId" as "thumbnailAssetId",
                  "album"."isActivityEnabled",
                  "album"."order",
                  "album"."updateId"::text as update_id
                FROM "album"
                LEFT JOIN "album_user" as "album_users" ON "album"."id" = "album_users"."albumId"
                WHERE "album"."updateId" < $1::uuid
                  AND "album_users"."userId" = $2
                ORDER BY "album"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "id": row.get::<Uuid, _>("id"),
                "name": row.get::<String, _>("name"),
                "description": row.get::<Option<String>, _>("description"),
                "createdAt": dt(row.get("createdAt")),
                "updatedAt": dt(row.get("updatedAt")),
                "thumbnailAssetId": row.get::<Option<Uuid>, _>("thumbnailAssetId"),
                "isActivityEnabled": row.get::<bool, _>("isActivityEnabled"),
                "order": row.get::<String, _>("order"),
            }),
        })
        .collect())
}

pub async fn album_get_album_users(
    pool: &PgPool,
    album_id: &Uuid,
) -> Result<Vec<AlbumUserRow>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT "userId", role FROM "album_user" WHERE "albumId" = $1"#,
    )
    .bind(album_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| AlbumUserRow {
            user_id: row.get("userId"),
            role: row.get("role"),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Album asset
// ---------------------------------------------------------------------------

fn map_album_asset_row(row: &sqlx::postgres::PgRow) -> SyncUpsert {
    SyncUpsert {
        update_id: row.get("update_id"),
        data: map_asset_fields(
            row.get("id"),
            row.get("ownerId"),
            row.get("originalFileName"),
            row.get("thumbhash"),
            row.get("checksum"),
            row.get("fileCreatedAt"),
            row.get("fileModifiedAt"),
            row.get("createdAt"),
            row.get("localDateTime"),
            row.get("type"),
            row.get("deletedAt"),
            row.get("visibility"),
            row.get("duration"),
            row.get("livePhotoVideoId"),
            row.get("stackId"),
            row.get("libraryId"),
            row.get("width"),
            row.get("height"),
            row.get("isEdited"),
            row.get("isFavorite"),
        ),
    }
}

fn map_asset_upsert(row: &sqlx::postgres::PgRow, update_id_col: &str) -> SyncUpsert {
    SyncUpsert {
        update_id: row.get(update_id_col),
        data: map_asset_fields(
            row.get("id"),
            row.get("ownerId"),
            row.get("originalFileName"),
            row.get("thumbhash"),
            row.get("checksum"),
            row.get("fileCreatedAt"),
            row.get("fileModifiedAt"),
            row.get("createdAt"),
            row.get("localDateTime"),
            row.get("type"),
            row.get("deletedAt"),
            row.get("visibility"),
            row.get("duration"),
            row.get("livePhotoVideoId"),
            row.get("stackId"),
            row.get("libraryId"),
            row.get("width"),
            row.get("height"),
            row.get("isEdited"),
            row.get("isFavorite"),
        ),
    }
}

fn map_partner_asset_row(row: &sqlx::postgres::PgRow) -> SyncUpsert {
    map_asset_upsert(row, "update_id")
}

fn map_stack_upsert(row: &sqlx::postgres::PgRow) -> SyncUpsert {
    SyncUpsert {
        update_id: row.get("update_id"),
        data: json!({
            "id": row.get::<Uuid, _>("id"),
            "createdAt": dt(row.get("createdAt")),
            "updatedAt": dt(row.get("updatedAt")),
            "primaryAssetId": row.get::<Option<Uuid>, _>("primaryAssetId"),
            "ownerId": row.get::<Uuid, _>("ownerId"),
        }),
    }
}

fn map_user_data(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<Uuid, _>("id"),
        "name": row.get::<String, _>("name"),
        "email": row.get::<String, _>("email"),
        "avatarColor": row.get::<String, _>("avatarColor"),
        "deletedAt": opt_dt(row.get("deletedAt")),
        "profileImagePath": row.get::<Option<String>, _>("profileImagePath"),
        "profileChangedAt": opt_dt(row.get("profileChangedAt")),
    })
}

const ALBUM_ASSET_SELECT: &str = r#"
  "asset"."id",
  "asset"."ownerId",
  "asset"."originalFileName",
  "asset"."thumbhash",
  "asset"."checksum",
  "asset"."fileCreatedAt",
  "asset"."fileModifiedAt",
  "asset"."createdAt",
  "asset"."localDateTime",
  "asset"."type",
  "asset"."deletedAt",
  "asset"."visibility",
  "asset"."duration",
  "asset"."livePhotoVideoId",
  "asset"."stackId",
  "asset"."libraryId",
  "asset"."width",
  "asset"."height",
  "asset"."isEdited",
  CASE WHEN "asset"."ownerId" = $1 THEN "asset"."isFavorite" ELSE false END as "isFavorite"
"#;

pub async fn album_asset_get_backfill(
    pool: &PgPool,
    options: &SyncBackfillOptions,
    album_id: &Uuid,
    user_id: &Uuid,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.after_update_id {
        Some(after) => {
            sqlx::query(&format!(
                r#"
                SELECT {ALBUM_ASSET_SELECT}, "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                INNER JOIN "asset" ON "asset"."id" = "album_asset"."assetId"
                WHERE "album_asset"."updateId" < $2::uuid
                  AND "album_asset"."updateId" <= $3::uuid
                  AND "album_asset"."updateId" > $4::uuid
                  AND "album_asset"."albumId" = $5
                ORDER BY "album_asset"."updateId" ASC
                "#
            ))
            .bind(user_id)
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(after)
            .bind(album_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {ALBUM_ASSET_SELECT}, "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                INNER JOIN "asset" ON "asset"."id" = "album_asset"."assetId"
                WHERE "album_asset"."updateId" < $2::uuid
                  AND "album_asset"."updateId" <= $3::uuid
                  AND "album_asset"."albumId" = $4
                ORDER BY "album_asset"."updateId" ASC
                "#
            ))
            .bind(user_id)
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(album_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(|row| map_album_asset_row(&row)).collect())
}

pub async fn album_asset_get_updates(
    pool: &PgPool,
    options: &SyncQueryOptions,
    create_checkpoint: &SyncAck,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(&format!(
                r#"
                SELECT {ALBUM_ASSET_SELECT}, "asset"."updateId"::text as update_id
                FROM "asset"
                INNER JOIN "album_asset" ON "album_asset"."assetId" = "asset"."id"
                INNER JOIN "album_user" ON "album_user"."albumId" = "album_asset"."albumId"
                WHERE "asset"."updateId" < $2::uuid
                  AND "asset"."updateId" > $3::uuid
                  AND "album_asset"."updateId" <= $4::uuid
                  AND "album_user"."userId" = $5
                ORDER BY "asset"."updateId" ASC
                "#
            ))
            .bind(&options.user_id)
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(&create_checkpoint.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {ALBUM_ASSET_SELECT}, "asset"."updateId"::text as update_id
                FROM "asset"
                INNER JOIN "album_asset" ON "album_asset"."assetId" = "asset"."id"
                INNER JOIN "album_user" ON "album_user"."albumId" = "album_asset"."albumId"
                WHERE "asset"."updateId" < $2::uuid
                  AND "album_asset"."updateId" <= $3::uuid
                  AND "album_user"."userId" = $4
                ORDER BY "asset"."updateId" ASC
                "#
            ))
            .bind(&options.user_id)
            .bind(&options.now_id)
            .bind(&create_checkpoint.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(|row| map_album_asset_row(&row)).collect())
}

pub async fn album_asset_get_creates(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(&format!(
                r#"
                SELECT {ALBUM_ASSET_SELECT}, "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                INNER JOIN "asset" ON "asset"."id" = "album_asset"."assetId"
                INNER JOIN "album_user" ON "album_user"."albumId" = "album_asset"."albumId"
                WHERE "album_asset"."updateId" < $2::uuid
                  AND "album_asset"."updateId" > $3::uuid
                  AND "album_user"."userId" = $4
                ORDER BY "album_asset"."updateId" ASC
                "#
            ))
            .bind(&options.user_id)
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {ALBUM_ASSET_SELECT}, "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                INNER JOIN "asset" ON "asset"."id" = "album_asset"."assetId"
                INNER JOIN "album_user" ON "album_user"."albumId" = "album_asset"."albumId"
                WHERE "album_asset"."updateId" < $2::uuid
                  AND "album_user"."userId" = $3
                ORDER BY "album_asset"."updateId" ASC
                "#
            ))
            .bind(&options.user_id)
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(|row| map_album_asset_row(&row)).collect())
}

// ---------------------------------------------------------------------------
// Album asset exif
// ---------------------------------------------------------------------------

const ASSET_EXIF_SELECT: &str = r#"
  "asset_exif"."assetId",
  "asset_exif"."description",
  "asset_exif"."exifImageWidth",
  "asset_exif"."exifImageHeight",
  "asset_exif"."fileSizeInByte",
  "asset_exif"."orientation",
  "asset_exif"."dateTimeOriginal",
  "asset_exif"."modifyDate",
  "asset_exif"."timeZone",
  "asset_exif"."latitude",
  "asset_exif"."longitude",
  "asset_exif"."projectionType",
  "asset_exif"."city",
  "asset_exif"."state",
  "asset_exif"."country",
  "asset_exif"."make",
  "asset_exif"."model",
  "asset_exif"."lensModel",
  "asset_exif"."fNumber",
  "asset_exif"."focalLength",
  "asset_exif"."iso",
  "asset_exif"."exposureTime",
  "asset_exif"."profileDescription",
  "asset_exif"."rating",
  "asset_exif"."fps"
"#;

pub async fn album_asset_exif_get_backfill(
    pool: &PgPool,
    options: &SyncBackfillOptions,
    album_id: &Uuid,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.after_update_id {
        Some(after) => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                INNER JOIN "asset_exif" ON "asset_exif"."assetId" = "album_asset"."assetId"
                WHERE "album_asset"."updateId" < $1::uuid
                  AND "album_asset"."updateId" <= $2::uuid
                  AND "album_asset"."updateId" > $3::uuid
                  AND "album_asset"."albumId" = $4
                ORDER BY "album_asset"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(after)
            .bind(album_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                INNER JOIN "asset_exif" ON "asset_exif"."assetId" = "album_asset"."assetId"
                WHERE "album_asset"."updateId" < $1::uuid
                  AND "album_asset"."updateId" <= $2::uuid
                  AND "album_asset"."albumId" = $3
                ORDER BY "album_asset"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(album_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| map_asset_exif_upsert(&row, "update_id"))
        .collect())
}

pub async fn album_asset_exif_get_updates(
    pool: &PgPool,
    options: &SyncQueryOptions,
    create_checkpoint: &SyncAck,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "asset_exif"."updateId"::text as update_id
                FROM "asset_exif"
                INNER JOIN "album_asset" ON "album_asset"."assetId" = "asset_exif"."assetId"
                INNER JOIN "album_user" ON "album_user"."albumId" = "album_asset"."albumId"
                WHERE "asset_exif"."updateId" < $1::uuid
                  AND "asset_exif"."updateId" > $2::uuid
                  AND "album_asset"."updateId" <= $3::uuid
                  AND "album_user"."userId" = $4
                ORDER BY "asset_exif"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(&create_checkpoint.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "asset_exif"."updateId"::text as update_id
                FROM "asset_exif"
                INNER JOIN "album_asset" ON "album_asset"."assetId" = "asset_exif"."assetId"
                INNER JOIN "album_user" ON "album_user"."albumId" = "album_asset"."albumId"
                WHERE "asset_exif"."updateId" < $1::uuid
                  AND "album_asset"."updateId" <= $2::uuid
                  AND "album_user"."userId" = $3
                ORDER BY "asset_exif"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&create_checkpoint.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| map_asset_exif_upsert(&row, "update_id"))
        .collect())
}

pub async fn album_asset_exif_get_creates(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                INNER JOIN "asset_exif" ON "asset_exif"."assetId" = "album_asset"."assetId"
                INNER JOIN "album" ON "album"."id" = "album_asset"."albumId"
                LEFT JOIN "album_user" ON "album_user"."albumId" = "album_asset"."albumId"
                WHERE "album_asset"."updateId" < $1::uuid
                  AND "album_asset"."updateId" > $2::uuid
                  AND "album_user"."userId" = $3
                ORDER BY "album_asset"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                INNER JOIN "asset_exif" ON "asset_exif"."assetId" = "album_asset"."assetId"
                INNER JOIN "album" ON "album"."id" = "album_asset"."albumId"
                LEFT JOIN "album_user" ON "album_user"."albumId" = "album_asset"."albumId"
                WHERE "album_asset"."updateId" < $1::uuid
                  AND "album_user"."userId" = $2
                ORDER BY "album_asset"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| map_asset_exif_upsert(&row, "update_id"))
        .collect())
}
// ---------------------------------------------------------------------------
// Album to asset
// ---------------------------------------------------------------------------

pub async fn album_to_asset_get_backfill(
    pool: &PgPool,
    options: &SyncBackfillOptions,
    album_id: &Uuid,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.after_update_id {
        Some(after) => {
            sqlx::query(
                r#"
                SELECT "album_asset"."assetId" as "assetId",
                  "album_asset"."albumId" as "albumId",
                  "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                WHERE "album_asset"."updateId" < $1::uuid
                  AND "album_asset"."updateId" <= $2::uuid
                  AND "album_asset"."updateId" > $3::uuid
                  AND "album_asset"."albumId" = $4
                ORDER BY "album_asset"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(after)
            .bind(album_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "album_asset"."assetId" as "assetId",
                  "album_asset"."albumId" as "albumId",
                  "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                WHERE "album_asset"."updateId" < $1::uuid
                  AND "album_asset"."updateId" <= $2::uuid
                  AND "album_asset"."albumId" = $3
                ORDER BY "album_asset"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(album_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "assetId": row.get::<Uuid, _>("assetId"),
                "albumId": row.get::<Uuid, _>("albumId"),
            }),
        })
        .collect())
}

pub async fn album_to_asset_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "assetId", "albumId"
                FROM "album_asset_audit"
                WHERE "album_asset_audit"."id" < $1::uuid
                  AND "album_asset_audit"."id" > $2::uuid
                  AND "albumId" IN (
                    SELECT "album_user"."albumId" as id
                    FROM "album_user"
                    WHERE "album_user"."userId" = $3
                  )
                ORDER BY "album_asset_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "assetId", "albumId"
                FROM "album_asset_audit"
                WHERE "album_asset_audit"."id" < $1::uuid
                  AND "albumId" IN (
                    SELECT "album_user"."albumId" as id
                    FROM "album_user"
                    WHERE "album_user"."userId" = $2
                  )
                ORDER BY "album_asset_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({
                "assetId": row.get::<Uuid, _>("assetId"),
                "albumId": row.get::<Uuid, _>("albumId"),
            }),
        })
        .collect())
}

pub async fn album_to_asset_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "album_asset"."assetId" as "assetId",
                  "album_asset"."albumId" as "albumId",
                  "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                INNER JOIN "album_user" ON "album_user"."albumId" = "album_asset"."albumId"
                WHERE "album_asset"."updateId" < $1::uuid
                  AND "album_asset"."updateId" > $2::uuid
                  AND "album_user"."userId" = $3
                ORDER BY "album_asset"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "album_asset"."assetId" as "assetId",
                  "album_asset"."albumId" as "albumId",
                  "album_asset"."updateId"::text as update_id
                FROM "album_asset"
                INNER JOIN "album_user" ON "album_user"."albumId" = "album_asset"."albumId"
                WHERE "album_asset"."updateId" < $1::uuid
                  AND "album_user"."userId" = $2
                ORDER BY "album_asset"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "assetId": row.get::<Uuid, _>("assetId"),
                "albumId": row.get::<Uuid, _>("albumId"),
            }),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Album user
// ---------------------------------------------------------------------------

pub async fn album_user_get_backfill(
    pool: &PgPool,
    options: &SyncBackfillOptions,
    album_id: &Uuid,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.after_update_id {
        Some(after) => {
            sqlx::query(
                r#"
                SELECT "album_user"."albumId" as "albumId",
                  "album_user"."userId" as "userId",
                  "album_user"."role",
                  "album_user"."updateId"::text as update_id
                FROM "album_user"
                WHERE "album_user"."updateId" < $1::uuid
                  AND "album_user"."updateId" <= $2::uuid
                  AND "album_user"."updateId" > $3::uuid
                  AND "albumId" = $4
                ORDER BY "album_user"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(after)
            .bind(album_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "album_user"."albumId" as "albumId",
                  "album_user"."userId" as "userId",
                  "album_user"."role",
                  "album_user"."updateId"::text as update_id
                FROM "album_user"
                WHERE "album_user"."updateId" < $1::uuid
                  AND "album_user"."updateId" <= $2::uuid
                  AND "albumId" = $3
                ORDER BY "album_user"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(album_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "albumId": row.get::<Uuid, _>("albumId"),
                "userId": row.get::<Uuid, _>("userId"),
                "role": row.get::<String, _>("role"),
            }),
        })
        .collect())
}

pub async fn album_user_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "userId", "albumId"
                FROM "album_user_audit"
                WHERE "album_user_audit"."id" < $1::uuid
                  AND "album_user_audit"."id" > $2::uuid
                  AND "albumId" IN (
                    SELECT "album_user"."albumId" as id
                    FROM "album_user"
                    WHERE "album_user"."userId" = $3
                  )
                ORDER BY "album_user_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "userId", "albumId"
                FROM "album_user_audit"
                WHERE "album_user_audit"."id" < $1::uuid
                  AND "albumId" IN (
                    SELECT "album_user"."albumId" as id
                    FROM "album_user"
                    WHERE "album_user"."userId" = $2
                  )
                ORDER BY "album_user_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({
                "userId": row.get::<Uuid, _>("userId"),
                "albumId": row.get::<Uuid, _>("albumId"),
            }),
        })
        .collect())
}

pub async fn album_user_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "album_user"."albumId" as "albumId",
                  "album_user"."userId" as "userId",
                  "album_user"."role",
                  "album_user"."updateId"::text as update_id
                FROM "album_user"
                WHERE "album_user"."updateId" < $1::uuid
                  AND "album_user"."updateId" > $2::uuid
                  AND "album_user"."albumId" IN (
                    SELECT "albumUsers"."albumId" as id
                    FROM "album_user" as "albumUsers"
                    WHERE "albumUsers"."userId" = $3
                  )
                ORDER BY "album_user"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "album_user"."albumId" as "albumId",
                  "album_user"."userId" as "userId",
                  "album_user"."role",
                  "album_user"."updateId"::text as update_id
                FROM "album_user"
                WHERE "album_user"."updateId" < $1::uuid
                  AND "album_user"."albumId" IN (
                    SELECT "albumUsers"."albumId" as id
                    FROM "album_user" as "albumUsers"
                    WHERE "albumUsers"."userId" = $2
                  )
                ORDER BY "album_user"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "albumId": row.get::<Uuid, _>("albumId"),
                "userId": row.get::<Uuid, _>("userId"),
                "role": row.get::<String, _>("role"),
            }),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Asset
// ---------------------------------------------------------------------------

pub async fn asset_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "assetId"
                FROM "asset_audit"
                WHERE "asset_audit"."id" < $1::uuid
                  AND "asset_audit"."id" > $2::uuid
                  AND "ownerId" = $3
                ORDER BY "asset_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "assetId"
                FROM "asset_audit"
                WHERE "asset_audit"."id" < $1::uuid
                  AND "ownerId" = $2
                ORDER BY "asset_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({ "assetId": row.get::<Uuid, _>("assetId") }),
        })
        .collect())
}

pub async fn asset_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "asset"."id",
                  "asset"."ownerId",
                  "asset"."originalFileName",
                  "asset"."thumbhash",
                  "asset"."checksum",
                  "asset"."fileCreatedAt",
                  "asset"."fileModifiedAt",
                  "asset"."createdAt",
                  "asset"."localDateTime",
                  "asset"."type",
                  "asset"."deletedAt",
                  "asset"."isFavorite",
                  "asset"."visibility",
                  "asset"."duration",
                  "asset"."livePhotoVideoId",
                  "asset"."stackId",
                  "asset"."libraryId",
                  "asset"."width",
                  "asset"."height",
                  "asset"."isEdited",
                  "asset"."updateId"::text as update_id
                FROM "asset"
                WHERE "asset"."updateId" < $1::uuid
                  AND "asset"."updateId" > $2::uuid
                  AND "ownerId" = $3
                ORDER BY "asset"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "asset"."id",
                  "asset"."ownerId",
                  "asset"."originalFileName",
                  "asset"."thumbhash",
                  "asset"."checksum",
                  "asset"."fileCreatedAt",
                  "asset"."fileModifiedAt",
                  "asset"."createdAt",
                  "asset"."localDateTime",
                  "asset"."type",
                  "asset"."deletedAt",
                  "asset"."isFavorite",
                  "asset"."visibility",
                  "asset"."duration",
                  "asset"."livePhotoVideoId",
                  "asset"."stackId",
                  "asset"."libraryId",
                  "asset"."width",
                  "asset"."height",
                  "asset"."isEdited",
                  "asset"."updateId"::text as update_id
                FROM "asset"
                WHERE "asset"."updateId" < $1::uuid
                  AND "ownerId" = $2
                ORDER BY "asset"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| map_asset_upsert(&row, "update_id"))
        .collect())
}

// ---------------------------------------------------------------------------
// Asset exif
// ---------------------------------------------------------------------------

pub async fn asset_exif_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "asset_exif"."updateId"::text as update_id
                FROM "asset_exif"
                WHERE "asset_exif"."updateId" < $1::uuid
                  AND "asset_exif"."updateId" > $2::uuid
                  AND "assetId" IN (
                    SELECT id FROM "asset" WHERE "ownerId" = $3
                  )
                ORDER BY "asset_exif"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "asset_exif"."updateId"::text as update_id
                FROM "asset_exif"
                WHERE "asset_exif"."updateId" < $1::uuid
                  AND "assetId" IN (
                    SELECT id FROM "asset" WHERE "ownerId" = $2
                  )
                ORDER BY "asset_exif"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| map_asset_exif_upsert(&row, "update_id"))
        .collect())
}

// ---------------------------------------------------------------------------
// Asset edit
// ---------------------------------------------------------------------------

pub async fn asset_edit_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "asset_edit_audit"."id"::text, "editId"
                FROM "asset_edit_audit"
                INNER JOIN "asset" ON "asset"."id" = "asset_edit_audit"."assetId"
                WHERE "asset_edit_audit"."id" < $1::uuid
                  AND "asset_edit_audit"."id" > $2::uuid
                  AND "asset"."ownerId" = $3
                ORDER BY "asset_edit_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "asset_edit_audit"."id"::text, "editId"
                FROM "asset_edit_audit"
                INNER JOIN "asset" ON "asset"."id" = "asset_edit_audit"."assetId"
                WHERE "asset_edit_audit"."id" < $1::uuid
                  AND "asset"."ownerId" = $2
                ORDER BY "asset_edit_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({ "editId": row.get::<Uuid, _>("editId") }),
        })
        .collect())
}

pub async fn asset_edit_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "asset_edit"."id",
                  "asset_edit"."assetId",
                  "asset_edit"."sequence",
                  "asset_edit"."action",
                  "asset_edit"."parameters",
                  "asset_edit"."updateId"::text as update_id
                FROM "asset_edit"
                INNER JOIN "asset" ON "asset"."id" = "asset_edit"."assetId"
                WHERE "asset_edit"."updateId" < $1::uuid
                  AND "asset_edit"."updateId" > $2::uuid
                  AND "asset"."ownerId" = $3
                ORDER BY "asset_edit"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "asset_edit"."id",
                  "asset_edit"."assetId",
                  "asset_edit"."sequence",
                  "asset_edit"."action",
                  "asset_edit"."parameters",
                  "asset_edit"."updateId"::text as update_id
                FROM "asset_edit"
                INNER JOIN "asset" ON "asset"."id" = "asset_edit"."assetId"
                WHERE "asset_edit"."updateId" < $1::uuid
                  AND "asset"."ownerId" = $2
                ORDER BY "asset_edit"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "id": row.get::<Uuid, _>("id"),
                "assetId": row.get::<Uuid, _>("assetId"),
                "sequence": row.get::<i32, _>("sequence"),
                "action": row.get::<String, _>("action"),
                "parameters": row.get::<sqlx::types::Json<Value>, _>("parameters").0,
            }),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Asset face
// ---------------------------------------------------------------------------

pub async fn asset_face_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "asset_face_audit"."id"::text, "assetFaceId"
                FROM "asset_face_audit"
                LEFT JOIN "asset" ON "asset"."id" = "asset_face_audit"."assetId"
                WHERE "asset_face_audit"."id" < $1::uuid
                  AND "asset_face_audit"."id" > $2::uuid
                  AND "asset"."ownerId" = $3
                ORDER BY "asset_face_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "asset_face_audit"."id"::text, "assetFaceId"
                FROM "asset_face_audit"
                LEFT JOIN "asset" ON "asset"."id" = "asset_face_audit"."assetId"
                WHERE "asset_face_audit"."id" < $1::uuid
                  AND "asset"."ownerId" = $2
                ORDER BY "asset_face_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({ "assetFaceId": row.get::<Uuid, _>("assetFaceId") }),
        })
        .collect())
}

pub async fn asset_face_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let person_col = schema.sync_face_person_id_select("asset_face");
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(&format!(
                r#"
                SELECT "asset_face"."id",
                  "assetId",
                  {person_col},
                  "imageWidth",
                  "imageHeight",
                  "boundingBoxX1",
                  "boundingBoxY1",
                  "boundingBoxX2",
                  "boundingBoxY2",
                  "sourceType",
                  "isVisible",
                  "asset_face"."deletedAt",
                  "asset_face"."updateId"::text as update_id
                FROM "asset_face"
                LEFT JOIN "asset" ON "asset"."id" = "asset_face"."assetId"
                WHERE "asset_face"."updateId" < $1::uuid
                  AND "asset_face"."updateId" > $2::uuid
                  AND "asset"."ownerId" = $3
                ORDER BY "asset_face"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT "asset_face"."id",
                  "assetId",
                  {person_col},
                  "imageWidth",
                  "imageHeight",
                  "boundingBoxX1",
                  "boundingBoxY1",
                  "boundingBoxX2",
                  "boundingBoxY2",
                  "sourceType",
                  "isVisible",
                  "asset_face"."deletedAt",
                  "asset_face"."updateId"::text as update_id
                FROM "asset_face"
                LEFT JOIN "asset" ON "asset"."id" = "asset_face"."assetId"
                WHERE "asset_face"."updateId" < $1::uuid
                  AND "asset"."ownerId" = $2
                ORDER BY "asset_face"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "id": row.get::<Uuid, _>("id"),
                "assetId": row.get::<Uuid, _>("assetId"),
                "personId": row.get::<Option<Uuid>, _>("personId"),
                "imageWidth": row.get::<i32, _>("imageWidth"),
                "imageHeight": row.get::<i32, _>("imageHeight"),
                "boundingBoxX1": row.get::<f64, _>("boundingBoxX1"),
                "boundingBoxY1": row.get::<f64, _>("boundingBoxY1"),
                "boundingBoxX2": row.get::<f64, _>("boundingBoxX2"),
                "boundingBoxY2": row.get::<f64, _>("boundingBoxY2"),
                "sourceType": row.get::<String, _>("sourceType"),
                "isVisible": row.get::<bool, _>("isVisible"),
                "deletedAt": opt_dt(row.get("deletedAt")),
            }),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Asset metadata
// ---------------------------------------------------------------------------

pub async fn asset_metadata_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "asset_metadata_audit"."id"::text, "assetId", "key"
                FROM "asset_metadata_audit"
                LEFT JOIN "asset" ON "asset"."id" = "asset_metadata_audit"."assetId"
                WHERE "asset_metadata_audit"."id" < $1::uuid
                  AND "asset_metadata_audit"."id" > $2::uuid
                  AND "asset"."ownerId" = $3
                ORDER BY "asset_metadata_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "asset_metadata_audit"."id"::text, "assetId", "key"
                FROM "asset_metadata_audit"
                LEFT JOIN "asset" ON "asset"."id" = "asset_metadata_audit"."assetId"
                WHERE "asset_metadata_audit"."id" < $1::uuid
                  AND "asset"."ownerId" = $2
                ORDER BY "asset_metadata_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({
                "assetId": row.get::<Uuid, _>("assetId"),
                "key": row.get::<String, _>("key"),
            }),
        })
        .collect())
}

pub async fn asset_metadata_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "assetId", "key", "value", "asset_metadata"."updateId"::text as update_id
                FROM "asset_metadata"
                INNER JOIN "asset" ON "asset"."id" = "asset_metadata"."assetId"
                WHERE "asset_metadata"."updateId" < $1::uuid
                  AND "asset_metadata"."updateId" > $2::uuid
                  AND "asset"."ownerId" = $3
                ORDER BY "asset_metadata"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "assetId", "key", "value", "asset_metadata"."updateId"::text as update_id
                FROM "asset_metadata"
                INNER JOIN "asset" ON "asset"."id" = "asset_metadata"."assetId"
                WHERE "asset_metadata"."updateId" < $1::uuid
                  AND "asset"."ownerId" = $2
                ORDER BY "asset_metadata"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "assetId": row.get::<Uuid, _>("assetId"),
                "key": row.get::<String, _>("key"),
                "value": row.get::<sqlx::types::Json<Value>, _>("value").0,
            }),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Asset ocr
// ---------------------------------------------------------------------------

pub async fn asset_ocr_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "asset_ocr_audit"."id"::text,
                  "asset_ocr_audit"."assetId",
                  "asset_ocr_audit"."deletedAt"
                FROM "asset_ocr_audit"
                LEFT JOIN "asset" ON "asset"."id" = "asset_ocr_audit"."assetId"
                WHERE "asset_ocr_audit"."id" < $1::uuid
                  AND "asset_ocr_audit"."id" > $2::uuid
                  AND "asset"."ownerId" = $3
                ORDER BY "asset_ocr_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "asset_ocr_audit"."id"::text,
                  "asset_ocr_audit"."assetId",
                  "asset_ocr_audit"."deletedAt"
                FROM "asset_ocr_audit"
                LEFT JOIN "asset" ON "asset"."id" = "asset_ocr_audit"."assetId"
                WHERE "asset_ocr_audit"."id" < $1::uuid
                  AND "asset"."ownerId" = $2
                ORDER BY "asset_ocr_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({
                "assetId": row.get::<Uuid, _>("assetId"),
                "deletedAt": opt_dt(row.get("deletedAt")),
            }),
        })
        .collect())
}

pub async fn asset_ocr_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "asset_ocr"."id",
                  "asset_ocr"."assetId",
                  "asset_ocr"."x1",
                  "asset_ocr"."y1",
                  "asset_ocr"."x2",
                  "asset_ocr"."y2",
                  "asset_ocr"."x3",
                  "asset_ocr"."y3",
                  "asset_ocr"."x4",
                  "asset_ocr"."y4",
                  "asset_ocr"."text",
                  "asset_ocr"."boxScore",
                  "asset_ocr"."textScore",
                  "asset_ocr"."updateId"::text as update_id,
                  "asset_ocr"."isVisible"
                FROM "asset_ocr"
                INNER JOIN "asset" ON "asset"."id" = "asset_ocr"."assetId"
                WHERE "asset_ocr"."updateId" < $1::uuid
                  AND "asset_ocr"."updateId" > $2::uuid
                  AND "asset"."ownerId" = $3
                ORDER BY "asset_ocr"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "asset_ocr"."id",
                  "asset_ocr"."assetId",
                  "asset_ocr"."x1",
                  "asset_ocr"."y1",
                  "asset_ocr"."x2",
                  "asset_ocr"."y2",
                  "asset_ocr"."x3",
                  "asset_ocr"."y3",
                  "asset_ocr"."x4",
                  "asset_ocr"."y4",
                  "asset_ocr"."text",
                  "asset_ocr"."boxScore",
                  "asset_ocr"."textScore",
                  "asset_ocr"."updateId"::text as update_id,
                  "asset_ocr"."isVisible"
                FROM "asset_ocr"
                INNER JOIN "asset" ON "asset"."id" = "asset_ocr"."assetId"
                WHERE "asset_ocr"."updateId" < $1::uuid
                  AND "asset"."ownerId" = $2
                ORDER BY "asset_ocr"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "id": row.get::<Uuid, _>("id"),
                "assetId": row.get::<Uuid, _>("assetId"),
                "x1": row.get::<f64, _>("x1"),
                "y1": row.get::<f64, _>("y1"),
                "x2": row.get::<f64, _>("x2"),
                "y2": row.get::<f64, _>("y2"),
                "x3": row.get::<f64, _>("x3"),
                "y3": row.get::<f64, _>("y3"),
                "x4": row.get::<f64, _>("x4"),
                "y4": row.get::<f64, _>("y4"),
                "text": row.get::<String, _>("text"),
                "boxScore": row.get::<f64, _>("boxScore"),
                "textScore": row.get::<f64, _>("textScore"),
                "isVisible": row.get::<bool, _>("isVisible"),
            }),
        })
        .collect())
}
// ---------------------------------------------------------------------------
// Auth user
// ---------------------------------------------------------------------------

pub async fn auth_user_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id",
                  "name",
                  "email",
                  "avatarColor",
                  "deletedAt",
                  "updateId"::text as update_id,
                  "profileImagePath",
                  "profileChangedAt",
                  "isAdmin",
                  "pinCode",
                  "oauthId",
                  "storageLabel",
                  "quotaSizeInBytes",
                  "quotaUsageInBytes"
                FROM "user"
                WHERE "user"."updateId" < $1::uuid
                  AND "user"."updateId" > $2::uuid
                  AND "id" = $3
                ORDER BY "user"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id",
                  "name",
                  "email",
                  "avatarColor",
                  "deletedAt",
                  "updateId"::text as update_id,
                  "profileImagePath",
                  "profileChangedAt",
                  "isAdmin",
                  "pinCode",
                  "oauthId",
                  "storageLabel",
                  "quotaSizeInBytes",
                  "quotaUsageInBytes"
                FROM "user"
                WHERE "user"."updateId" < $1::uuid
                  AND "id" = $2
                ORDER BY "user"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "id": row.get::<Uuid, _>("id"),
                "name": row.get::<String, _>("name"),
                "email": row.get::<String, _>("email"),
                "avatarColor": row.get::<String, _>("avatarColor"),
                "deletedAt": opt_dt(row.get("deletedAt")),
                "profileImagePath": row.get::<Option<String>, _>("profileImagePath"),
                "profileChangedAt": opt_dt(row.get("profileChangedAt")),
                "isAdmin": row.get::<bool, _>("isAdmin"),
                "pinCode": row.get::<Option<String>, _>("pinCode"),
                "oauthId": row.get::<Option<String>, _>("oauthId"),
                "storageLabel": row.get::<Option<String>, _>("storageLabel"),
                "quotaSizeInBytes": row.get::<Option<i64>, _>("quotaSizeInBytes"),
                "quotaUsageInBytes": row.get::<i64, _>("quotaUsageInBytes"),
            }),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

pub async fn memory_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "memoryId"
                FROM "memory_audit"
                WHERE "memory_audit"."id" < $1::uuid
                  AND "memory_audit"."id" > $2::uuid
                  AND "userId" = $3
                ORDER BY "memory_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "memoryId"
                FROM "memory_audit"
                WHERE "memory_audit"."id" < $1::uuid
                  AND "userId" = $2
                ORDER BY "memory_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({ "memoryId": row.get::<Uuid, _>("memoryId") }),
        })
        .collect())
}

pub async fn memory_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id",
                  "createdAt",
                  "updatedAt",
                  "deletedAt",
                  "ownerId",
                  "type",
                  "data",
                  "isSaved",
                  "memoryAt",
                  "seenAt",
                  "showAt",
                  "hideAt",
                  "updateId"::text as update_id
                FROM "memory"
                WHERE "memory"."updateId" < $1::uuid
                  AND "memory"."updateId" > $2::uuid
                  AND "ownerId" = $3
                ORDER BY "memory"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id",
                  "createdAt",
                  "updatedAt",
                  "deletedAt",
                  "ownerId",
                  "type",
                  "data",
                  "isSaved",
                  "memoryAt",
                  "seenAt",
                  "showAt",
                  "hideAt",
                  "updateId"::text as update_id
                FROM "memory"
                WHERE "memory"."updateId" < $1::uuid
                  AND "ownerId" = $2
                ORDER BY "memory"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "id": row.get::<Uuid, _>("id"),
                "createdAt": dt(row.get("createdAt")),
                "updatedAt": dt(row.get("updatedAt")),
                "deletedAt": opt_dt(row.get("deletedAt")),
                "ownerId": row.get::<Uuid, _>("ownerId"),
                "type": row.get::<String, _>("type"),
                "data": row.get::<sqlx::types::Json<Value>, _>("data").0,
                "isSaved": row.get::<bool, _>("isSaved"),
                "memoryAt": dt(row.get("memoryAt")),
                "seenAt": opt_dt(row.get("seenAt")),
                "showAt": opt_dt(row.get("showAt")),
                "hideAt": opt_dt(row.get("hideAt")),
            }),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Memory to asset
// ---------------------------------------------------------------------------

pub async fn memory_to_asset_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "memoryId", "assetId"
                FROM "memory_asset_audit"
                WHERE "memory_asset_audit"."id" < $1::uuid
                  AND "memory_asset_audit"."id" > $2::uuid
                  AND "memoryId" IN (
                    SELECT id FROM "memory" WHERE "ownerId" = $3
                  )
                ORDER BY "memory_asset_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "memoryId", "assetId"
                FROM "memory_asset_audit"
                WHERE "memory_asset_audit"."id" < $1::uuid
                  AND "memoryId" IN (
                    SELECT id FROM "memory" WHERE "ownerId" = $2
                  )
                ORDER BY "memory_asset_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({
                "memoryId": row.get::<Uuid, _>("memoryId"),
                "assetId": row.get::<Uuid, _>("assetId"),
            }),
        })
        .collect())
}

pub async fn memory_to_asset_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "memoriesId" as "memoryId",
                  "assetId" as "assetId",
                  "updateId"::text as update_id
                FROM "memory_asset"
                WHERE "memory_asset"."updateId" < $1::uuid
                  AND "memory_asset"."updateId" > $2::uuid
                  AND "memoriesId" IN (
                    SELECT id FROM "memory" WHERE "ownerId" = $3
                  )
                ORDER BY "memory_asset"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "memoriesId" as "memoryId",
                  "assetId" as "assetId",
                  "updateId"::text as update_id
                FROM "memory_asset"
                WHERE "memory_asset"."updateId" < $1::uuid
                  AND "memoriesId" IN (
                    SELECT id FROM "memory" WHERE "ownerId" = $2
                  )
                ORDER BY "memory_asset"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "memoryId": row.get::<Uuid, _>("memoryId"),
                "assetId": row.get::<Uuid, _>("assetId"),
            }),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Partner
// ---------------------------------------------------------------------------

pub async fn partner_get_created_after(
    pool: &PgPool,
    options: &SyncCreatedAfterOptions,
) -> Result<Vec<SyncPartnerCreatedAfter>, sqlx::Error> {
    let rows = match &options.after_create_id {
        Some(after) => {
            sqlx::query(
                r#"
                SELECT "sharedById", "createId"::text as create_id
                FROM "partner"
                WHERE "sharedWithId" = $1
                  AND "createId" >= $2::uuid
                  AND "createId" < $3::uuid
                ORDER BY "partner"."createId" ASC
                "#,
            )
            .bind(options.user_id)
            .bind(after)
            .bind(&options.now_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "sharedById", "createId"::text as create_id
                FROM "partner"
                WHERE "sharedWithId" = $1
                  AND "createId" < $2::uuid
                ORDER BY "partner"."createId" ASC
                "#,
            )
            .bind(options.user_id)
            .bind(&options.now_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncPartnerCreatedAfter {
            shared_by_id: row.get("sharedById"),
            create_id: row.get("create_id"),
        })
        .collect())
}

pub async fn partner_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "sharedById", "sharedWithId"
                FROM "partner_audit"
                WHERE "partner_audit"."id" < $1::uuid
                  AND "partner_audit"."id" > $2::uuid
                  AND ("sharedById" = $3 OR "sharedWithId" = $4)
                ORDER BY "partner_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "sharedById", "sharedWithId"
                FROM "partner_audit"
                WHERE "partner_audit"."id" < $1::uuid
                  AND ("sharedById" = $2 OR "sharedWithId" = $3)
                ORDER BY "partner_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({
                "sharedById": row.get::<Uuid, _>("sharedById"),
                "sharedWithId": row.get::<Uuid, _>("sharedWithId"),
            }),
        })
        .collect())
}

pub async fn partner_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "sharedById",
                  "sharedWithId",
                  "inTimeline",
                  "updateId"::text as update_id
                FROM "partner"
                WHERE "partner"."updateId" < $1::uuid
                  AND "partner"."updateId" > $2::uuid
                  AND ("sharedById" = $3 OR "sharedWithId" = $4)
                ORDER BY "partner"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "sharedById",
                  "sharedWithId",
                  "inTimeline",
                  "updateId"::text as update_id
                FROM "partner"
                WHERE "partner"."updateId" < $1::uuid
                  AND ("sharedById" = $2 OR "sharedWithId" = $3)
                ORDER BY "partner"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "sharedById": row.get::<Uuid, _>("sharedById"),
                "sharedWithId": row.get::<Uuid, _>("sharedWithId"),
                "inTimeline": row.get::<bool, _>("inTimeline"),
            }),
        })
        .collect())
}
// ---------------------------------------------------------------------------
// Partner asset
// ---------------------------------------------------------------------------

const PARTNER_ASSET_SELECT: &str = r#"
  "asset"."id",
  "asset"."ownerId",
  "asset"."originalFileName",
  "asset"."thumbhash",
  "asset"."checksum",
  "asset"."fileCreatedAt",
  "asset"."fileModifiedAt",
  "asset"."localDateTime",
  "asset"."createdAt",
  "asset"."type",
  "asset"."deletedAt",
  "asset"."visibility",
  "asset"."duration",
  "asset"."livePhotoVideoId",
  "asset"."stackId",
  "asset"."libraryId",
  "asset"."width",
  "asset"."height",
  "asset"."isEdited",
  $1 as "isFavorite"
"#;

pub async fn partner_asset_get_backfill(
    pool: &PgPool,
    options: &SyncBackfillOptions,
    partner_id: &Uuid,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.after_update_id {
        Some(after) => {
            sqlx::query(&format!(
                r#"
                SELECT {PARTNER_ASSET_SELECT}, "asset"."updateId"::text as update_id
                FROM "asset"
                WHERE "asset"."updateId" < $2::uuid
                  AND "asset"."updateId" <= $3::uuid
                  AND "asset"."updateId" > $4::uuid
                  AND "ownerId" = $5
                ORDER BY "asset"."updateId" ASC
                "#
            ))
            .bind(false)
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(after)
            .bind(partner_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {PARTNER_ASSET_SELECT}, "asset"."updateId"::text as update_id
                FROM "asset"
                WHERE "asset"."updateId" < $2::uuid
                  AND "asset"."updateId" <= $3::uuid
                  AND "ownerId" = $4
                ORDER BY "asset"."updateId" ASC
                "#
            ))
            .bind(false)
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(partner_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(|row| map_partner_asset_row(&row)).collect())
}

pub async fn partner_asset_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "assetId"
                FROM "asset_audit"
                WHERE "asset_audit"."id" < $1::uuid
                  AND "asset_audit"."id" > $2::uuid
                  AND "ownerId" IN (
                    SELECT "sharedById" FROM "partner" WHERE "sharedWithId" = $3
                  )
                ORDER BY "asset_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "assetId"
                FROM "asset_audit"
                WHERE "asset_audit"."id" < $1::uuid
                  AND "ownerId" IN (
                    SELECT "sharedById" FROM "partner" WHERE "sharedWithId" = $2
                  )
                ORDER BY "asset_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({ "assetId": row.get::<Uuid, _>("assetId") }),
        })
        .collect())
}

pub async fn partner_asset_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(&format!(
                r#"
                SELECT {PARTNER_ASSET_SELECT}, "asset"."updateId"::text as update_id
                FROM "asset"
                WHERE "asset"."updateId" < $2::uuid
                  AND "asset"."updateId" > $3::uuid
                  AND "ownerId" IN (
                    SELECT "sharedById" FROM "partner" WHERE "sharedWithId" = $4
                  )
                ORDER BY "asset"."updateId" ASC
                "#
            ))
            .bind(false)
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {PARTNER_ASSET_SELECT}, "asset"."updateId"::text as update_id
                FROM "asset"
                WHERE "asset"."updateId" < $2::uuid
                  AND "ownerId" IN (
                    SELECT "sharedById" FROM "partner" WHERE "sharedWithId" = $3
                  )
                ORDER BY "asset"."updateId" ASC
                "#
            ))
            .bind(false)
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(|row| map_partner_asset_row(&row)).collect())
}

// ---------------------------------------------------------------------------
// Partner asset exif
// ---------------------------------------------------------------------------

pub async fn partner_asset_exif_get_backfill(
    pool: &PgPool,
    options: &SyncBackfillOptions,
    partner_id: &Uuid,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.after_update_id {
        Some(after) => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "asset_exif"."updateId"::text as update_id
                FROM "asset_exif"
                INNER JOIN "asset" ON "asset"."id" = "asset_exif"."assetId"
                WHERE "asset_exif"."updateId" < $1::uuid
                  AND "asset_exif"."updateId" <= $2::uuid
                  AND "asset_exif"."updateId" > $3::uuid
                  AND "asset"."ownerId" = $4
                ORDER BY "asset_exif"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(after)
            .bind(partner_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "asset_exif"."updateId"::text as update_id
                FROM "asset_exif"
                INNER JOIN "asset" ON "asset"."id" = "asset_exif"."assetId"
                WHERE "asset_exif"."updateId" < $1::uuid
                  AND "asset_exif"."updateId" <= $2::uuid
                  AND "asset"."ownerId" = $3
                ORDER BY "asset_exif"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(partner_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| map_asset_exif_upsert(&row, "update_id"))
        .collect())
}

pub async fn partner_asset_exif_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "asset_exif"."updateId"::text as update_id
                FROM "asset_exif"
                WHERE "asset_exif"."updateId" < $1::uuid
                  AND "asset_exif"."updateId" > $2::uuid
                  AND "assetId" IN (
                    SELECT id FROM "asset"
                    WHERE "ownerId" IN (
                      SELECT "sharedById" FROM "partner" WHERE "sharedWithId" = $3
                    )
                  )
                ORDER BY "asset_exif"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {ASSET_EXIF_SELECT}, "asset_exif"."updateId"::text as update_id
                FROM "asset_exif"
                WHERE "asset_exif"."updateId" < $1::uuid
                  AND "assetId" IN (
                    SELECT id FROM "asset"
                    WHERE "ownerId" IN (
                      SELECT "sharedById" FROM "partner" WHERE "sharedWithId" = $2
                    )
                  )
                ORDER BY "asset_exif"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| map_asset_exif_upsert(&row, "update_id"))
        .collect())
}

// ---------------------------------------------------------------------------
// Partner stack
// ---------------------------------------------------------------------------

pub async fn partner_stack_get_backfill(
    pool: &PgPool,
    options: &SyncBackfillOptions,
    partner_id: &Uuid,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.after_update_id {
        Some(after) => {
            sqlx::query(
                r#"
                SELECT "stack"."id",
                  "stack"."createdAt",
                  "stack"."updatedAt",
                  "stack"."primaryAssetId",
                  "stack"."ownerId",
                  "updateId"::text as update_id
                FROM "stack"
                WHERE "stack"."updateId" < $1::uuid
                  AND "stack"."updateId" <= $2::uuid
                  AND "stack"."updateId" > $3::uuid
                  AND "ownerId" = $4
                ORDER BY "stack"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(after)
            .bind(partner_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "stack"."id",
                  "stack"."createdAt",
                  "stack"."updatedAt",
                  "stack"."primaryAssetId",
                  "stack"."ownerId",
                  "updateId"::text as update_id
                FROM "stack"
                WHERE "stack"."updateId" < $1::uuid
                  AND "stack"."updateId" <= $2::uuid
                  AND "ownerId" = $3
                ORDER BY "stack"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&options.before_update_id)
            .bind(partner_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(|row| map_stack_upsert(&row)).collect())
}

pub async fn partner_stack_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "stackId"
                FROM "stack_audit"
                WHERE "stack_audit"."id" < $1::uuid
                  AND "stack_audit"."id" > $2::uuid
                  AND "userId" IN (
                    SELECT "sharedById" FROM "partner" WHERE "sharedWithId" = $3
                  )
                ORDER BY "stack_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "stackId"
                FROM "stack_audit"
                WHERE "stack_audit"."id" < $1::uuid
                  AND "userId" IN (
                    SELECT "sharedById" FROM "partner" WHERE "sharedWithId" = $2
                  )
                ORDER BY "stack_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({ "stackId": row.get::<Uuid, _>("stackId") }),
        })
        .collect())
}

pub async fn partner_stack_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "stack"."id",
                  "stack"."createdAt",
                  "stack"."updatedAt",
                  "stack"."primaryAssetId",
                  "stack"."ownerId",
                  "updateId"::text as update_id
                FROM "stack"
                WHERE "stack"."updateId" < $1::uuid
                  AND "stack"."updateId" > $2::uuid
                  AND "ownerId" IN (
                    SELECT "sharedById" FROM "partner" WHERE "sharedWithId" = $3
                  )
                ORDER BY "stack"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "stack"."id",
                  "stack"."createdAt",
                  "stack"."updatedAt",
                  "stack"."primaryAssetId",
                  "stack"."ownerId",
                  "updateId"::text as update_id
                FROM "stack"
                WHERE "stack"."updateId" < $1::uuid
                  AND "ownerId" IN (
                    SELECT "sharedById" FROM "partner" WHERE "sharedWithId" = $2
                  )
                ORDER BY "stack"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(|row| map_stack_upsert(&row)).collect())
}

// ---------------------------------------------------------------------------
// Person
// ---------------------------------------------------------------------------

pub async fn person_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let person_col = schema.audit_person_id_select();
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(&format!(
                r#"
                SELECT "id"::text, {person_col}
                FROM "person_audit"
                WHERE "person_audit"."id" < $1::uuid
                  AND "person_audit"."id" > $2::uuid
                  AND "ownerId" = $3
                ORDER BY "person_audit"."id" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT "id"::text, {person_col}
                FROM "person_audit"
                WHERE "person_audit"."id" < $1::uuid
                  AND "ownerId" = $2
                ORDER BY "person_audit"."id" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({ "personId": row.get::<Uuid, _>("personId") }),
        })
        .collect())
}

pub async fn person_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let schema = PersonSchema::get(pool).await?;
    let person_id = schema.person_id_as_id("");
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(&format!(
                r#"
                SELECT {person_id},
                  "createdAt",
                  "updatedAt",
                  "ownerId",
                  "name",
                  "birthDate",
                  "isHidden",
                  "isFavorite",
                  "color",
                  "updateId"::text as update_id,
                  "faceAssetId"
                FROM "person"
                WHERE "person"."updateId" < $1::uuid
                  AND "person"."updateId" > $2::uuid
                  AND "ownerId" = $3
                ORDER BY "person"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                r#"
                SELECT {person_id},
                  "createdAt",
                  "updatedAt",
                  "ownerId",
                  "name",
                  "birthDate",
                  "isHidden",
                  "isFavorite",
                  "color",
                  "updateId"::text as update_id,
                  "faceAssetId"
                FROM "person"
                WHERE "person"."updateId" < $1::uuid
                  AND "ownerId" = $2
                ORDER BY "person"."updateId" ASC
                "#
            ))
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "id": row.get::<Uuid, _>("id"),
                "createdAt": dt(row.get("createdAt")),
                "updatedAt": dt(row.get("updatedAt")),
                "ownerId": row.get::<Uuid, _>("ownerId"),
                "name": row.get::<String, _>("name"),
                "birthDate": opt_date(row.get("birthDate")),
                "isHidden": row.get::<bool, _>("isHidden"),
                "isFavorite": row.get::<bool, _>("isFavorite"),
                "color": row.get::<String, _>("color"),
                "faceAssetId": row.get::<Option<Uuid>, _>("faceAssetId"),
            }),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Stack
// ---------------------------------------------------------------------------

pub async fn stack_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "stackId"
                FROM "stack_audit"
                WHERE "stack_audit"."id" < $1::uuid
                  AND "stack_audit"."id" > $2::uuid
                  AND "userId" = $3
                ORDER BY "stack_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "stackId"
                FROM "stack_audit"
                WHERE "stack_audit"."id" < $1::uuid
                  AND "userId" = $2
                ORDER BY "stack_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({ "stackId": row.get::<Uuid, _>("stackId") }),
        })
        .collect())
}

pub async fn stack_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "stack"."id",
                  "stack"."createdAt",
                  "stack"."updatedAt",
                  "stack"."primaryAssetId",
                  "stack"."ownerId",
                  "updateId"::text as update_id
                FROM "stack"
                WHERE "stack"."updateId" < $1::uuid
                  AND "stack"."updateId" > $2::uuid
                  AND "ownerId" = $3
                ORDER BY "stack"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "stack"."id",
                  "stack"."createdAt",
                  "stack"."updatedAt",
                  "stack"."primaryAssetId",
                  "stack"."ownerId",
                  "updateId"::text as update_id
                FROM "stack"
                WHERE "stack"."updateId" < $1::uuid
                  AND "ownerId" = $2
                ORDER BY "stack"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(|row| map_stack_upsert(&row)).collect())
}

// ---------------------------------------------------------------------------
// User
// ---------------------------------------------------------------------------

pub async fn user_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "userId"
                FROM "user_audit"
                WHERE "user_audit"."id" < $1::uuid
                  AND "user_audit"."id" > $2::uuid
                ORDER BY "user_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "userId"
                FROM "user_audit"
                WHERE "user_audit"."id" < $1::uuid
                ORDER BY "user_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({ "userId": row.get::<Uuid, _>("userId") }),
        })
        .collect())
}

pub async fn user_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id",
                  "name",
                  "email",
                  "avatarColor",
                  "deletedAt",
                  "updateId"::text as update_id,
                  "profileImagePath",
                  "profileChangedAt"
                FROM "user"
                WHERE "user"."updateId" < $1::uuid
                  AND "user"."updateId" > $2::uuid
                ORDER BY "user"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id",
                  "name",
                  "email",
                  "avatarColor",
                  "deletedAt",
                  "updateId"::text as update_id,
                  "profileImagePath",
                  "profileChangedAt"
                FROM "user"
                WHERE "user"."updateId" < $1::uuid
                ORDER BY "user"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: map_user_data(&row),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// User metadata
// ---------------------------------------------------------------------------

pub async fn user_metadata_get_deletes(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncDelete>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "id"::text, "userId", "key"
                FROM "user_metadata_audit"
                WHERE "user_metadata_audit"."id" < $1::uuid
                  AND "user_metadata_audit"."id" > $2::uuid
                  AND "userId" = $3
                ORDER BY "user_metadata_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "id"::text, "userId", "key"
                FROM "user_metadata_audit"
                WHERE "user_metadata_audit"."id" < $1::uuid
                  AND "userId" = $2
                ORDER BY "user_metadata_audit"."id" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncDelete {
            audit_id: row.get("id"),
            data: json!({
                "userId": row.get::<Uuid, _>("userId"),
                "key": row.get::<String, _>("key"),
            }),
        })
        .collect())
}

pub async fn user_metadata_get_upserts(
    pool: &PgPool,
    options: &SyncQueryOptions,
) -> Result<Vec<SyncUpsert>, sqlx::Error> {
    let rows = match &options.ack {
        Some(ack) => {
            sqlx::query(
                r#"
                SELECT "userId", "key", "value", "updateId"::text as update_id
                FROM "user_metadata"
                WHERE "user_metadata"."updateId" < $1::uuid
                  AND "user_metadata"."updateId" > $2::uuid
                  AND "userId" = $3
                ORDER BY "user_metadata"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(&ack.update_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT "userId", "key", "value", "updateId"::text as update_id
                FROM "user_metadata"
                WHERE "user_metadata"."updateId" < $1::uuid
                  AND "userId" = $2
                ORDER BY "user_metadata"."updateId" ASC
                "#,
            )
            .bind(&options.now_id)
            .bind(options.user_id)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| SyncUpsert {
            update_id: row.get("update_id"),
            data: json!({
                "userId": row.get::<Uuid, _>("userId"),
                "key": row.get::<String, _>("key"),
                "value": row.get::<sqlx::types::Json<Value>, _>("value").0,
            }),
        })
        .collect())
}
