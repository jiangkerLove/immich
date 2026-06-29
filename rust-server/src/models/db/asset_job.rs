use serde::Deserialize;
use serde_json::Value;
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

use crate::models::db::asset_edit::AssetEditRow;

#[derive(Debug, Clone, FromRow, Deserialize)]
pub struct AssetFileJobRow {
    pub id: Uuid,
    pub path: String,
    pub file_type: String,
    pub is_edited: bool,
    pub is_progressive: bool,
    pub is_transparent: bool,
}

#[derive(Debug, Clone)]
pub struct ThumbnailAssetJob {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub visibility: String,
    pub original_file_name: String,
    pub original_path: String,
    pub asset_type: String,
    pub is_edited: bool,
    pub thumbhash: Option<Vec<u8>>,
    pub orientation: Option<String>,
    pub projection_type: Option<String>,
    pub exif_image_width: Option<i32>,
    pub exif_image_height: Option<i32>,
    pub video_index: Option<i32>,
    pub video_codec_name: Option<String>,
    pub pixel_format: Option<String>,
    pub color_primaries: Option<i16>,
    pub color_transfer: Option<i16>,
    pub color_matrix: Option<i16>,
    pub format_name: Option<String>,
    pub files: Vec<AssetFileJobRow>,
    pub edits: Vec<AssetEditRow>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ThumbnailQueueAsset {
    pub id: Uuid,
    pub is_edited: bool,
}

const WEB_UNSUPPORTED_EXTENSIONS: &[&str] = &[
    ".3fr", ".ari", ".arw", ".cap", ".cin", ".cr2", ".cr3", ".crw", ".dcr", ".dng", ".erf", ".fff",
    ".iiq", ".k25", ".kdc", ".mrw", ".nef", ".nrw", ".orf", ".ori", ".pef", ".psd", ".raf", ".raw",
    ".rw2", ".rwl", ".sr2", ".srf", ".srw", ".x3f", ".heic", ".heif", ".hif", ".insp", ".jp2",
    ".jpe", ".jxl", ".mpo", ".svg", ".tif", ".tiff",
];

pub async fn get_for_generate_thumbnail(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<ThumbnailAssetJob>, sqlx::Error> {
    let row = sqlx::query_as::<_, ThumbnailAssetQueryRow>(
        r#"
        SELECT
            asset.id,
            asset.visibility,
            asset."originalFileName" AS original_file_name,
            asset."originalPath" AS original_path,
            asset."ownerId" AS owner_id,
            asset."isEdited" AS is_edited,
            asset.thumbhash,
            asset.type AS asset_type,
            asset_exif.orientation,
            asset_exif."projectionType" AS projection_type,
            asset_exif."exifImageWidth" AS exif_image_width,
            asset_exif."exifImageHeight" AS exif_image_height,
            asset_video.index AS video_index,
            asset_video."codecName" AS video_codec_name,
            asset_video."pixelFormat" AS pixel_format,
            asset_video."colorPrimaries" AS color_primaries,
            asset_video."colorTransfer" AS color_transfer,
            asset_video."colorMatrix" AS color_matrix,
            asset_video."formatName" AS format_name,
            COALESCE(
                (
                    SELECT json_agg(row_to_json(f))
                    FROM (
                        SELECT
                            af.id,
                            af.path,
                            af.type AS file_type,
                            af."isEdited" AS is_edited,
                            af."isProgressive" AS is_progressive,
                            af."isTransparent" AS is_transparent
                        FROM asset_file af
                        WHERE af."assetId" = asset.id
                          AND af.type IN ('preview', 'thumbnail', 'fullsize')
                    ) f
                ),
                '[]'::json
            ) AS files_json,
            COALESCE(
                (
                    SELECT json_agg(row_to_json(e))
                    FROM (
                        SELECT id, action, parameters
                        FROM asset_edit ae
                        WHERE ae."assetId" = asset.id
                        ORDER BY ae.sequence ASC
                    ) e
                ),
                '[]'::json
            ) AS edits_json
        FROM asset
        LEFT JOIN asset_exif ON asset_exif."assetId" = asset.id
        LEFT JOIN asset_video ON asset_video."assetId" = asset.id
        WHERE asset.id = $1
          AND asset."deletedAt" IS NULL
          AND asset_exif."assetId" IS NOT NULL
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    row.map(|row| row.into_job()).transpose()
}

pub async fn stream_for_thumbnail_job(
    pool: &Pool<Postgres>,
    force: bool,
    fullsize_enabled: bool,
) -> Result<Vec<ThumbnailQueueAsset>, sqlx::Error> {
    if force {
        return sqlx::query_as::<_, ThumbnailQueueAsset>(
            r#"
            SELECT asset.id, asset."isEdited" AS is_edited
            FROM asset
            WHERE asset."deletedAt" IS NULL
              AND asset.visibility != 'hidden'
            ORDER BY asset.id
            "#,
        )
        .fetch_all(pool)
        .await;
    }

    let fullsize_clause = if fullsize_enabled {
        format!(
            r#"
            OR (
                NOT EXISTS (
                    SELECT FROM asset_file
                    WHERE "assetId" = asset.id AND type = 'fullsize'
                )
                AND f_unaccent(asset."originalFileName") LIKE ANY (
                    ARRAY[{}]::text[]
                )
            )"#,
            WEB_UNSUPPORTED_EXTENSIONS
                .iter()
                .map(|ext| format!("'%{ext}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        String::new()
    };

    let query = format!(
        r#"
        SELECT asset.id, asset."isEdited" AS is_edited
        FROM asset
        INNER JOIN asset_job_status ON asset_job_status."assetId" = asset.id
        WHERE asset."deletedAt" IS NULL
          AND asset.visibility != 'hidden'
          AND (
            NOT EXISTS (
                SELECT FROM asset_file
                WHERE "assetId" = asset.id AND type = 'thumbnail'
            )
            OR NOT EXISTS (
                SELECT FROM asset_file
                WHERE "assetId" = asset.id AND type = 'preview'
            )
            OR (
                asset."isEdited" = true
                AND NOT EXISTS (
                    SELECT FROM asset_file
                    WHERE "assetId" = asset.id
                      AND type = 'fullsize'
                      AND "isEdited" = true
                )
            )
            OR asset.thumbhash IS NULL
            {fullsize_clause}
          )
        ORDER BY asset.id
        "#
    );

    sqlx::query_as::<_, ThumbnailQueueAsset>(&query)
        .fetch_all(pool)
        .await
}

#[derive(Debug, FromRow)]
struct ThumbnailAssetQueryRow {
    id: Uuid,
    visibility: String,
    original_file_name: String,
    original_path: String,
    owner_id: Uuid,
    is_edited: bool,
    thumbhash: Option<Vec<u8>>,
    asset_type: String,
    orientation: Option<String>,
    projection_type: Option<String>,
    exif_image_width: Option<i32>,
    exif_image_height: Option<i32>,
    video_index: Option<i32>,
    video_codec_name: Option<String>,
    pixel_format: Option<String>,
    color_primaries: Option<i16>,
    color_transfer: Option<i16>,
    color_matrix: Option<i16>,
    format_name: Option<String>,
    files_json: Value,
    edits_json: Value,
}

impl ThumbnailAssetQueryRow {
    fn into_job(self) -> Result<ThumbnailAssetJob, sqlx::Error> {
        let files: Vec<AssetFileJobRow> =
            serde_json::from_value(self.files_json).map_err(|err| {
                sqlx::Error::Decode(Box::new(err))
            })?;
        let edits: Vec<AssetEditRow> = serde_json::from_value(self.edits_json).map_err(|err| {
            sqlx::Error::Decode(Box::new(err))
        })?;
        Ok(ThumbnailAssetJob {
            id: self.id,
            owner_id: self.owner_id,
            visibility: self.visibility,
            original_file_name: self.original_file_name,
            original_path: self.original_path,
            asset_type: self.asset_type,
            is_edited: self.is_edited,
            thumbhash: self.thumbhash,
            orientation: self.orientation,
            projection_type: self.projection_type,
            exif_image_width: self.exif_image_width,
            exif_image_height: self.exif_image_height,
            video_index: self.video_index,
            video_codec_name: self.video_codec_name,
            pixel_format: self.pixel_format,
            color_primaries: self.color_primaries,
            color_transfer: self.color_transfer,
            color_matrix: self.color_matrix,
            format_name: self.format_name,
            files,
            edits,
        })
    }
}

#[derive(Debug)]
pub struct UpsertAssetFile {
    pub asset_id: Uuid,
    pub path: String,
    pub file_type: String,
    pub is_edited: bool,
    pub is_progressive: bool,
    pub is_transparent: bool,
}

pub async fn upsert_asset_files(
    pool: &Pool<Postgres>,
    files: &[UpsertAssetFile],
) -> Result<(), sqlx::Error> {
    for file in files {
        sqlx::query(
            r#"
            INSERT INTO asset_file ("assetId", type, path, "isEdited", "isProgressive", "isTransparent")
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT ("assetId", type, "isEdited")
            DO UPDATE SET
                path = EXCLUDED.path,
                "isProgressive" = EXCLUDED."isProgressive",
                "isTransparent" = EXCLUDED."isTransparent",
                "updatedAt" = NOW()
            "#,
        )
        .bind(file.asset_id)
        .bind(&file.file_type)
        .bind(&file.path)
        .bind(file.is_edited)
        .bind(file.is_progressive)
        .bind(file.is_transparent)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn update_thumbhash(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    thumbhash: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE asset SET thumbhash = $1 WHERE id = $2"#)
        .bind(thumbhash)
        .bind(asset_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_asset_dimensions(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    width: i32,
    height: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE asset SET width = $1, height = $2 WHERE id = $3"#)
        .bind(width)
        .bind(height)
        .bind(asset_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_job_status_thumbnails(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO asset_job_status ("assetId", "thumbnailsGeneratedAt")
        VALUES ($1, NOW())
        ON CONFLICT ("assetId")
        DO UPDATE SET "thumbnailsGeneratedAt" = EXCLUDED."thumbnailsGeneratedAt"
        "#,
    )
    .bind(asset_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, FromRow)]
pub struct PersonThumbnailJobData {
    pub owner_id: Uuid,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub old_width: i32,
    pub old_height: i32,
    pub asset_type: String,
    pub original_path: String,
    pub exif_orientation: Option<String>,
    pub preview_path: Option<String>,
}

pub async fn get_person_thumbnail_job_data(
    pool: &Pool<Postgres>,
    person_id: &Uuid,
) -> Result<Option<PersonThumbnailJobData>, sqlx::Error> {
    sqlx::query_as::<_, PersonThumbnailJobData>(
        r#"
        SELECT
            person."ownerId" AS owner_id,
            asset_face."boundingBoxX1" AS x1,
            asset_face."boundingBoxY1" AS y1,
            asset_face."boundingBoxX2" AS x2,
            asset_face."boundingBoxY2" AS y2,
            asset_face."imageWidth" AS old_width,
            asset_face."imageHeight" AS old_height,
            asset.type AS asset_type,
            asset."originalPath" AS original_path,
            asset_exif.orientation AS exif_orientation,
            (
                SELECT asset_file.path
                FROM asset_file
                WHERE asset_file."assetId" = asset.id
                  AND asset_file.type = 'preview'
                  AND asset_file."isEdited" = false
                LIMIT 1
            ) AS preview_path
        FROM person
        INNER JOIN asset_face ON asset_face.id = person."faceAssetId"
        INNER JOIN asset ON asset_face."assetId" = asset.id
        LEFT JOIN asset_exif ON asset_exif."assetId" = asset.id
        WHERE person.id = $1
          AND asset_face."deletedAt" IS NULL
        "#,
    )
    .bind(person_id)
    .fetch_optional(pool)
    .await
}

pub async fn update_person_thumbnail_path(
    pool: &Pool<Postgres>,
    person_id: &Uuid,
    thumbnail_path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE person SET "thumbnailPath" = $1 WHERE id = $2"#)
        .bind(thumbnail_path)
        .bind(person_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_random_face_id(
    pool: &Pool<Postgres>,
    person_id: &Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT asset_face.id
        FROM asset_face
        WHERE asset_face."personId" = $1
          AND asset_face."deletedAt" IS NULL
          AND asset_face."isVisible" = true
        LIMIT 1
        "#,
    )
    .bind(person_id)
    .fetch_optional(pool)
    .await
}

pub async fn update_person_face_asset_id(
    pool: &Pool<Postgres>,
    person_id: &Uuid,
    face_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE person SET "faceAssetId" = $1 WHERE id = $2"#)
        .bind(face_id)
        .bind(person_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn stream_people_for_thumbnail_job(
    pool: &Pool<Postgres>,
    force: bool,
) -> Result<Vec<PersonThumbnailQueueRow>, sqlx::Error> {
    if force {
        sqlx::query_as::<_, PersonThumbnailQueueRow>(
            r#"
            SELECT id, "faceAssetId" AS face_asset_id
            FROM person
            ORDER BY id
            "#,
        )
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, PersonThumbnailQueueRow>(
            r#"
            SELECT id, "faceAssetId" AS face_asset_id
            FROM person
            WHERE "thumbnailPath" = ''
            ORDER BY id
            "#,
        )
        .fetch_all(pool)
        .await
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct PersonThumbnailQueueRow {
    pub id: Uuid,
    pub face_asset_id: Option<Uuid>,
}

#[derive(Debug, Clone, FromRow, Deserialize)]
pub struct VideoConversionFileRow {
    pub id: Uuid,
    pub path: String,
    pub file_type: String,
    pub is_edited: bool,
}

#[derive(Debug, Clone)]
pub struct VideoConversionJob {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub original_path: String,
    pub video_index: i32,
    pub video_codec_name: String,
    pub video_bitrate: i64,
    pub video_width: i32,
    pub video_height: i32,
    pub pixel_format: String,
    pub frame_count: i64,
    pub frame_rate: Option<f64>,
    pub rotation: i32,
    pub color_transfer: Option<i16>,
    pub format_name: String,
    pub format_long_name: Option<String>,
    pub audio_index: Option<i32>,
    pub audio_codec_name: Option<String>,
    pub files: Vec<VideoConversionFileRow>,
}

pub async fn get_for_video_conversion(
    pool: &Pool<Postgres>,
    id: &Uuid,
) -> Result<Option<VideoConversionJob>, sqlx::Error> {
    let row = sqlx::query_as::<_, VideoConversionQueryRow>(
        r#"
        SELECT
            asset.id,
            asset."ownerId" AS owner_id,
            asset."originalPath" AS original_path,
            asset_video.index AS video_index,
            asset_video."codecName" AS video_codec_name,
            asset_video.bitrate AS video_bitrate,
            asset_exif."exifImageWidth" AS video_width,
            asset_exif."exifImageHeight" AS video_height,
            asset_video."pixelFormat" AS pixel_format,
            asset_video."frameCount" AS frame_count,
            asset_exif.fps AS frame_rate,
            CASE
                WHEN asset_exif.orientation = '6' THEN -90
                WHEN asset_exif.orientation = '8' THEN 90
                WHEN asset_exif.orientation = '3' THEN 180
                ELSE 0
            END AS rotation,
            asset_video."colorTransfer" AS color_transfer,
            asset_video."formatName" AS format_name,
            asset_video."formatLongName" AS format_long_name,
            asset_audio.index AS audio_index,
            asset_audio."codecName" AS audio_codec_name,
            COALESCE(
                (
                    SELECT json_agg(row_to_json(f))
                    FROM (
                        SELECT
                            af.id,
                            af.path,
                            af.type AS file_type,
                            af."isEdited" AS is_edited
                        FROM asset_file af
                        WHERE af."assetId" = asset.id
                    ) f
                ),
                '[]'::json
            ) AS files_json
        FROM asset
        INNER JOIN asset_exif ON asset_exif."assetId" = asset.id
        INNER JOIN asset_video ON asset_video."assetId" = asset.id
        LEFT JOIN asset_audio ON asset_audio."assetId" = asset.id
        WHERE asset.id = $1
          AND asset.type = 'VIDEO'
          AND asset."deletedAt" IS NULL
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    row.map(|row| row.into_job()).transpose()
}

pub async fn stream_for_video_conversion(
    pool: &Pool<Postgres>,
    force: bool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if force {
        return sqlx::query_scalar(
            r#"
            SELECT asset.id
            FROM asset
            WHERE asset.type = 'VIDEO'
              AND asset."deletedAt" IS NULL
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
        WHERE asset.type = 'VIDEO'
          AND asset."deletedAt" IS NULL
          AND asset.visibility != 'hidden'
          AND NOT EXISTS (
              SELECT 1
              FROM asset_file
              WHERE "assetId" = asset.id
                AND type = 'encoded_video'
          )
        ORDER BY asset.id
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn delete_asset_file_by_id(
    pool: &Pool<Postgres>,
    file_id: &Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM asset_file WHERE id = $1"#)
        .bind(file_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, FromRow)]
struct VideoConversionQueryRow {
    id: Uuid,
    owner_id: Uuid,
    original_path: String,
    video_index: i32,
    video_codec_name: String,
    video_bitrate: i64,
    video_width: i32,
    video_height: i32,
    pixel_format: String,
    frame_count: i64,
    frame_rate: Option<f64>,
    rotation: i32,
    color_transfer: Option<i16>,
    format_name: String,
    format_long_name: Option<String>,
    audio_index: Option<i32>,
    audio_codec_name: Option<String>,
    files_json: Value,
}

impl VideoConversionQueryRow {
    fn into_job(self) -> Result<VideoConversionJob, sqlx::Error> {
        let files: Vec<VideoConversionFileRow> =
            serde_json::from_value(self.files_json).map_err(|err| {
                sqlx::Error::Decode(Box::new(err))
            })?;
        Ok(VideoConversionJob {
            id: self.id,
            owner_id: self.owner_id,
            original_path: self.original_path,
            video_index: self.video_index,
            video_codec_name: self.video_codec_name,
            video_bitrate: self.video_bitrate,
            video_width: self.video_width,
            video_height: self.video_height,
            pixel_format: self.pixel_format,
            frame_count: self.frame_count,
            frame_rate: self.frame_rate,
            rotation: self.rotation,
            color_transfer: self.color_transfer,
            format_name: self.format_name,
            format_long_name: self.format_long_name,
            audio_index: self.audio_index,
            audio_codec_name: self.audio_codec_name,
            files,
        })
    }
}
