use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct VideoStreamAssetRow {
    pub original_path: String,
    pub owner_id: Uuid,
    pub video_index: i16,
    pub codec_name: String,
    pub width: i32,
    pub height: i32,
    pub time_base: i32,
    pub frame_count: i32,
    pub frame_rate: Option<f64>,
    pub orientation: Option<i32>,
    pub pixel_format: String,
    pub color_transfer: i32,
    pub packet_count: i32,
    pub output_frames: i32,
    pub total_duration: i32,
    pub audio_index: Option<i16>,
}

#[derive(Debug, FromRow)]
pub struct VideoStreamSessionRow {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

const VIDEO_STREAM_SELECT: &str = r#"
    asset."originalPath" AS original_path,
    asset."ownerId" AS owner_id,
    asset_video.index AS video_index,
    asset_video."codecName" AS codec_name,
    asset_exif."exifImageWidth" AS width,
    asset_exif."exifImageHeight" AS height,
    asset_video."timeBase" AS time_base,
    asset_video."frameCount" AS frame_count,
    asset_exif.fps AS frame_rate,
    asset_exif.orientation AS orientation,
    asset_video."pixelFormat" AS pixel_format,
    asset_video."colorTransfer" AS color_transfer,
    asset_keyframe."packetCount" AS packet_count,
    asset_keyframe."outputFrames" AS output_frames,
    asset_keyframe."totalDuration" AS total_duration,
    asset_audio.index AS audio_index
"#;

const VIDEO_STREAM_JOINS: &str = r#"
    INNER JOIN asset_exif ON asset.id = asset_exif."assetId"
    INNER JOIN asset_video ON asset.id = asset_video."assetId"
    INNER JOIN asset_keyframe ON asset.id = asset_keyframe."assetId"
    LEFT JOIN asset_audio ON asset.id = asset_audio."assetId"
"#;

pub async fn get_for_main_playlist(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<VideoStreamAssetRow>, sqlx::Error> {
    sqlx::query_as::<_, VideoStreamAssetRow>(&format!(
        r#"
            SELECT {VIDEO_STREAM_SELECT}
            FROM asset
            {VIDEO_STREAM_JOINS}
            WHERE asset.id = $1
        "#
    ))
    .bind(asset_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_for_media_playlist(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
    session_id: &Uuid,
) -> Result<Option<VideoStreamAssetRow>, sqlx::Error> {
    sqlx::query_as::<_, VideoStreamAssetRow>(&format!(
        r#"
            SELECT {VIDEO_STREAM_SELECT}
            FROM asset
            INNER JOIN video_stream_session ON asset.id = video_stream_session."assetId"
            {VIDEO_STREAM_JOINS}
            WHERE asset.id = $1
              AND video_stream_session.id = $2
              AND video_stream_session."expiresAt" > NOW()
        "#
    ))
    .bind(asset_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_for_transcoding(
    pool: &Pool<Postgres>,
    asset_id: &Uuid,
) -> Result<Option<VideoStreamAssetRow>, sqlx::Error> {
    get_for_main_playlist(pool, asset_id).await
}

pub async fn create_session(
    pool: &Pool<Postgres>,
    session_id: &Uuid,
    asset_id: &Uuid,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
            INSERT INTO video_stream_session (id, "assetId", "expiresAt")
            VALUES ($1, $2, $3)
        "#,
    )
    .bind(session_id)
    .bind(asset_id)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_session(
    pool: &Pool<Postgres>,
    session_id: &Uuid,
) -> Result<Option<VideoStreamSessionRow>, sqlx::Error> {
    sqlx::query_as::<_, VideoStreamSessionRow>(
        r#"
            SELECT id, "assetId" AS asset_id, "expiresAt" AS expires_at
            FROM video_stream_session
            WHERE id = $1
              AND "expiresAt" > NOW()
        "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
}

pub async fn extend_session(
    pool: &Pool<Postgres>,
    session_id: &Uuid,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE video_stream_session SET "expiresAt" = $2 WHERE id = $1"#,
    )
    .bind(session_id)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_session(pool: &Pool<Postgres>, session_id: &Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM video_stream_session WHERE id = $1"#)
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub fn is_video_rotated(orientation: Option<i32>) -> bool {
    matches!(orientation, Some(6) | Some(8))
}

pub fn is_video_vertical(width: i32, height: i32, orientation: Option<i32>) -> bool {
    height > width || is_video_rotated(orientation)
}

pub fn output_size(width: i32, height: i32, orientation: Option<i32>, target_res: u32) -> (u32, u32) {
    let factor = (height.max(width) as f64) / (height.min(width).max(1) as f64);
    let mut larger = (target_res as f64 * factor).round() as u32;
    if larger % 2 != 0 {
        larger -= 1;
    }
    if is_video_vertical(width, height, orientation) {
        (target_res, larger)
    } else {
        (larger, target_res)
    }
}

pub fn segmentation(asset: &VideoStreamAssetRow) -> (f64, i32, i32, f64) {
    let fps = (asset.packet_count as f64 * asset.time_base as f64) / asset.total_duration.max(1) as f64;
    let frames_per_segment = (crate::utils::hls::HLS_SEGMENT_DURATION * fps).ceil() as i32;
    let segment_count = ((asset.output_frames as f64) / frames_per_segment.max(1) as f64).ceil() as i32;
    let segment_duration = frames_per_segment as f64 / fps.max(0.001);
    (fps, frames_per_segment, segment_count, segment_duration)
}
