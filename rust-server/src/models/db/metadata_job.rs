use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct MetadataExtractionAsset {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub asset_type: String,
    pub original_path: String,
    pub original_file_name: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub is_edited: bool,
    pub file_created_at: Option<DateTime<Utc>>,
    pub file_modified_at: Option<DateTime<Utc>>,
    pub sidecar_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpsertAssetExif {
    pub asset_id: Uuid,
    pub make: Option<String>,
    pub model: Option<String>,
    pub exif_image_width: Option<i32>,
    pub exif_image_height: Option<i32>,
    pub file_size_in_byte: Option<i64>,
    pub orientation: Option<String>,
    pub date_time_original: Option<DateTime<Utc>>,
    pub modify_date: Option<DateTime<Utc>>,
    pub lens_model: Option<String>,
    pub f_number: Option<f64>,
    pub focal_length: Option<f64>,
    pub iso: Option<i32>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub description: String,
    pub fps: Option<f64>,
    pub exposure_time: Option<String>,
    pub live_photo_cid: Option<String>,
    pub time_zone: Option<String>,
    pub projection_type: Option<String>,
    pub profile_description: Option<String>,
    pub colorspace: Option<String>,
    pub bits_per_sample: Option<i32>,
    pub auto_stack_id: Option<String>,
    pub rating: Option<i32>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct UpsertAssetVideo {
    pub asset_id: Uuid,
    pub bitrate: i32,
    pub frame_count: i32,
    pub time_base: i32,
    pub index: i16,
    pub profile: Option<i16>,
    pub level: Option<i16>,
    pub color_primaries: i16,
    pub color_transfer: i16,
    pub color_matrix: i16,
    pub codec_name: String,
    pub format_name: String,
    pub format_long_name: String,
    pub pixel_format: String,
}

#[derive(Debug, Clone)]
pub struct UpsertAssetAudio {
    pub asset_id: Uuid,
    pub bitrate: i32,
    pub index: i16,
    pub profile: Option<i16>,
    pub codec_name: String,
}

#[derive(Debug, Clone)]
pub struct UpdateAssetAfterMetadata {
    pub asset_id: Uuid,
    pub duration: Option<i64>,
    pub local_date_time: Option<DateTime<Utc>>,
    pub file_created_at: Option<DateTime<Utc>>,
    pub file_modified_at: Option<DateTime<Utc>>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

pub async fn get_for_metadata_extraction(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<MetadataExtractionAsset>, sqlx::Error> {
    let row = sqlx::query_as::<_, MetadataExtractionQueryRow>(
        r#"
        SELECT
            asset.id,
            asset."ownerId" AS owner_id,
            asset.type AS asset_type,
            asset."originalPath" AS original_path,
            asset."originalFileName" AS original_file_name,
            asset.width,
            asset.height,
            asset."isEdited" AS is_edited,
            asset."fileCreatedAt" AS file_created_at,
            asset."fileModifiedAt" AS file_modified_at,
            (
                SELECT af.path
                FROM asset_file af
                WHERE af."assetId" = asset.id
                  AND af.type = 'sidecar'
                  AND af."isEdited" = false
                LIMIT 1
            ) AS sidecar_path
        FROM asset
        WHERE asset.id = $1
          AND asset."deletedAt" IS NULL
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| MetadataExtractionAsset {
        id: row.id,
        owner_id: row.owner_id,
        asset_type: row.asset_type,
        original_path: row.original_path,
        original_file_name: row.original_file_name,
        width: row.width,
        height: row.height,
        is_edited: row.is_edited,
        file_created_at: row.file_created_at,
        file_modified_at: row.file_modified_at,
        sidecar_path: row.sidecar_path,
    }))
}

pub async fn stream_for_metadata_extraction(
    pool: &Pool<Postgres>,
    force: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if force {
        return sqlx::query_scalar(
            r#"
            SELECT asset.id
            FROM asset
            WHERE asset."deletedAt" IS NULL
            ORDER BY asset.id
            "#,
        )
        .fetch_all(pool)
        .await;
    }

    sqlx::query_scalar(
        r#"
        SELECT asset.id
        FROM asset
        LEFT JOIN asset_job_status ON asset_job_status."assetId" = asset.id
        WHERE asset."deletedAt" IS NULL
          AND (
            asset_job_status."metadataExtractedAt" IS NULL
            OR asset_job_status."assetId" IS NULL
          )
        ORDER BY asset.id
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn upsert_metadata(
    pool: &Pool<Postgres>,
    exif: &UpsertAssetExif,
    video: Option<&UpsertAssetVideo>,
    audio: Option<&UpsertAssetAudio>,
    asset_update: &UpdateAssetAfterMetadata,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO asset_exif (
            "assetId", make, model, "exifImageWidth", "exifImageHeight", "fileSizeInByte",
            orientation, "dateTimeOriginal", "modifyDate", "lensModel", "fNumber", "focalLength",
            iso, latitude, longitude, city, state, country, description, fps, "exposureTime",
            "livePhotoCID", "timeZone", "projectionType", "profileDescription", colorspace,
            "bitsPerSample", "autoStackId", rating, tags
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
            $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30
        )
        ON CONFLICT ("assetId") DO UPDATE SET
            make = EXCLUDED.make,
            model = EXCLUDED.model,
            "exifImageWidth" = EXCLUDED."exifImageWidth",
            "exifImageHeight" = EXCLUDED."exifImageHeight",
            "fileSizeInByte" = EXCLUDED."fileSizeInByte",
            orientation = EXCLUDED.orientation,
            "dateTimeOriginal" = EXCLUDED."dateTimeOriginal",
            "modifyDate" = EXCLUDED."modifyDate",
            "lensModel" = EXCLUDED."lensModel",
            "fNumber" = EXCLUDED."fNumber",
            "focalLength" = EXCLUDED."focalLength",
            iso = EXCLUDED.iso,
            latitude = EXCLUDED.latitude,
            longitude = EXCLUDED.longitude,
            city = EXCLUDED.city,
            state = EXCLUDED.state,
            country = EXCLUDED.country,
            description = EXCLUDED.description,
            fps = EXCLUDED.fps,
            "exposureTime" = EXCLUDED."exposureTime",
            "livePhotoCID" = EXCLUDED."livePhotoCID",
            "timeZone" = EXCLUDED."timeZone",
            "projectionType" = EXCLUDED."projectionType",
            "profileDescription" = EXCLUDED."profileDescription",
            colorspace = EXCLUDED.colorspace,
            "bitsPerSample" = EXCLUDED."bitsPerSample",
            "autoStackId" = EXCLUDED."autoStackId",
            rating = EXCLUDED.rating,
            tags = EXCLUDED.tags,
            "updatedAt" = NOW()
        "#,
    )
    .bind(exif.asset_id)
    .bind(&exif.make)
    .bind(&exif.model)
    .bind(exif.exif_image_width)
    .bind(exif.exif_image_height)
    .bind(exif.file_size_in_byte)
    .bind(&exif.orientation)
    .bind(exif.date_time_original)
    .bind(exif.modify_date)
    .bind(&exif.lens_model)
    .bind(exif.f_number)
    .bind(exif.focal_length)
    .bind(exif.iso)
    .bind(exif.latitude)
    .bind(exif.longitude)
    .bind(&exif.city)
    .bind(&exif.state)
    .bind(&exif.country)
    .bind(&exif.description)
    .bind(exif.fps)
    .bind(&exif.exposure_time)
    .bind(&exif.live_photo_cid)
    .bind(&exif.time_zone)
    .bind(&exif.projection_type)
    .bind(&exif.profile_description)
    .bind(&exif.colorspace)
    .bind(exif.bits_per_sample)
    .bind(&exif.auto_stack_id)
    .bind(exif.rating)
    .bind(exif.tags.as_ref().map(|tags| tags.as_slice()))
    .execute(&mut *tx)
    .await?;

    if let Some(video) = video {
        sqlx::query(
            r#"
            INSERT INTO asset_video (
                "assetId", bitrate, "frameCount", "timeBase", index, profile, level,
                "colorPrimaries", "colorTransfer", "colorMatrix", "codecName",
                "formatName", "formatLongName", "pixelFormat"
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ON CONFLICT ("assetId") DO UPDATE SET
                bitrate = EXCLUDED.bitrate,
                "frameCount" = EXCLUDED."frameCount",
                "timeBase" = EXCLUDED."timeBase",
                index = EXCLUDED.index,
                profile = EXCLUDED.profile,
                level = EXCLUDED.level,
                "colorPrimaries" = EXCLUDED."colorPrimaries",
                "colorTransfer" = EXCLUDED."colorTransfer",
                "colorMatrix" = EXCLUDED."colorMatrix",
                "codecName" = EXCLUDED."codecName",
                "formatName" = EXCLUDED."formatName",
                "formatLongName" = EXCLUDED."formatLongName",
                "pixelFormat" = EXCLUDED."pixelFormat"
            "#,
        )
        .bind(video.asset_id)
        .bind(video.bitrate)
        .bind(video.frame_count)
        .bind(video.time_base)
        .bind(video.index)
        .bind(video.profile)
        .bind(video.level)
        .bind(video.color_primaries)
        .bind(video.color_transfer)
        .bind(video.color_matrix)
        .bind(&video.codec_name)
        .bind(&video.format_name)
        .bind(&video.format_long_name)
        .bind(&video.pixel_format)
        .execute(&mut *tx)
        .await?;
    }

    if let Some(audio) = audio {
        sqlx::query(
            r#"
            INSERT INTO asset_audio ("assetId", bitrate, index, profile, "codecName")
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT ("assetId") DO UPDATE SET
                bitrate = EXCLUDED.bitrate,
                index = EXCLUDED.index,
                profile = EXCLUDED.profile,
                "codecName" = EXCLUDED."codecName"
            "#,
        )
        .bind(audio.asset_id)
        .bind(audio.bitrate)
        .bind(audio.index)
        .bind(audio.profile)
        .bind(&audio.codec_name)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        r#"
        UPDATE asset SET
            duration = COALESCE($2, duration),
            "localDateTime" = COALESCE($3, "localDateTime"),
            "fileCreatedAt" = COALESCE($4, "fileCreatedAt"),
            "fileModifiedAt" = COALESCE($5, "fileModifiedAt"),
            width = CASE WHEN $6 THEN COALESCE($7, width) ELSE width END,
            height = CASE WHEN $6 THEN COALESCE($8, height) ELSE height END
        WHERE id = $1
        "#,
    )
    .bind(asset_update.asset_id)
    .bind(asset_update.duration)
    .bind(asset_update.local_date_time)
    .bind(asset_update.file_created_at)
    .bind(asset_update.file_modified_at)
    .bind(asset_update.width.is_some() || asset_update.height.is_some())
    .bind(asset_update.width)
    .bind(asset_update.height)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO asset_job_status ("assetId", "metadataExtractedAt")
        VALUES ($1, NOW())
        ON CONFLICT ("assetId")
        DO UPDATE SET "metadataExtractedAt" = EXCLUDED."metadataExtractedAt"
        "#,
    )
    .bind(asset_update.asset_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

#[derive(Debug, FromRow)]
struct MetadataExtractionQueryRow {
    id: Uuid,
    owner_id: Uuid,
    asset_type: String,
    original_path: String,
    original_file_name: String,
    width: Option<i32>,
    height: Option<i32>,
    is_edited: bool,
    file_created_at: Option<DateTime<Utc>>,
    file_modified_at: Option<DateTime<Utc>>,
    sidecar_path: Option<String>,
}
