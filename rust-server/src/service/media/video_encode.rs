use std::path::Path;
use std::process::Stdio;

use serde_json::Value;
use sqlx::PgPool;
use tokio::process::Command;
use uuid::Uuid;

use crate::models::db::asset_job::{
    self, UpsertAssetFile, VideoConversionFileRow, VideoConversionJob,
};
use crate::models::db::system_metadata::get_json;
use crate::service::job::JobService;
use crate::utils::storage::StoragePaths;

const JOBS_BATCH_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoEncodeOutcome {
    Success,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranscodeTarget {
    None,
    Video,
    Audio,
    All,
}

#[derive(Debug, Clone)]
struct FfmpegConfig {
    crf: u32,
    threads: i32,
    preset: String,
    target_video_codec: String,
    accepted_video_codecs: Vec<String>,
    target_audio_codec: String,
    accepted_audio_codecs: Vec<String>,
    accepted_containers: Vec<String>,
    target_resolution: String,
    max_bitrate: String,
    bframes: i32,
    refs: u32,
    gop_size: u32,
    transcode: String,
    tonemap: String,
    accel: String,
    two_pass: bool,
}

impl Default for FfmpegConfig {
    fn default() -> Self {
        Self {
            crf: 23,
            threads: 0,
            preset: "ultrafast".into(),
            target_video_codec: "h264".into(),
            accepted_video_codecs: vec!["h264".into()],
            target_audio_codec: "aac".into(),
            accepted_audio_codecs: vec!["aac".into(), "mp3".into(), "opus".into()],
            accepted_containers: vec!["mov".into(), "ogg".into(), "webm".into()],
            target_resolution: "720".into(),
            max_bitrate: "0".into(),
            bframes: -1,
            refs: 0,
            gop_size: 0,
            transcode: "required".into(),
            tonemap: "hable".into(),
            accel: "disabled".into(),
            two_pass: false,
        }
    }
}

#[derive(Clone)]
pub struct VideoEncodeService {
    pool: PgPool,
    storage: StoragePaths,
    jobs: JobService,
}

impl VideoEncodeService {
    pub fn new(pool: PgPool, storage: StoragePaths, jobs: JobService) -> Self {
        Self { pool, storage, jobs }
    }

    pub async fn encode_asset_video(&self, asset_id: &Uuid) -> Result<VideoEncodeOutcome, String> {
        let Some(asset) = asset_job::get_for_video_conversion(&self.pool, asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Ok(VideoEncodeOutcome::Failed);
        };

        if asset.video_width <= 0 || asset.video_height <= 0 {
            eprintln!(
                "skipped video encoding for {}: missing video dimensions",
                asset.id
            );
            return Ok(VideoEncodeOutcome::Failed);
        }

        if !Path::new(&asset.original_path).exists() {
            return Ok(VideoEncodeOutcome::Failed);
        }

        let config = self.load_ffmpeg_config().await?;
        let target = get_transcode_target(&config, &asset);
        let remux_required = is_remux_required(&config, &asset);

        if target == TranscodeTarget::None && !remux_required {
            return self.handle_skip_no_transcode(&asset).await;
        }

        if config.accel != "disabled" {
            eprintln!(
                "hardware acceleration ({}) is not supported in rust-server video worker; using software encoding for {}",
                config.accel, asset.id
            );
        }

        let output = self.storage.encoded_video_path(&asset.owner_id, &asset.id);
        StoragePaths::ensure_parent(&output).map_err(|err| err.to_string())?;

        let args = build_ffmpeg_args(&asset.original_path, &output, target, remux_required, &config, &asset)?;
        run_ffmpeg(&args).await?;

        asset_job::upsert_asset_files(
            &self.pool,
            &[UpsertAssetFile {
                asset_id: asset.id,
                path: output.to_string_lossy().into_owned(),
                file_type: "encoded_video".into(),
                is_edited: false,
                is_progressive: false,
                is_transparent: false,
            }],
        )
        .await
        .map_err(|err| err.to_string())?;

        self.cleanup_stale_encoded_files(&asset, &output)
            .await?;

        Ok(VideoEncodeOutcome::Success)
    }

    pub async fn queue_all_video_encoding(&self, force: bool) -> Result<(), String> {
        let asset_ids = asset_job::stream_for_video_conversion(&self.pool, force)
            .await
            .map_err(|err| err.to_string())?;

        for chunk in asset_ids.chunks(JOBS_BATCH_SIZE) {
            for asset_id in chunk {
                self.jobs
                    .queue_asset_encode_video(asset_id)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }
        Ok(())
    }

    async fn handle_skip_no_transcode(
        &self,
        asset: &VideoConversionJob,
    ) -> Result<VideoEncodeOutcome, String> {
        if let Some(encoded) = find_encoded_video_file(&asset.files) {
            eprintln!(
                "encoded video exists for {} but is no longer required; deleting",
                asset.id
            );
            self.jobs
                .queue_file_delete(&[&encoded.path])
                .await
                .map_err(|err| err.to_string())?;
            asset_job::delete_asset_file_by_id(&self.pool, &encoded.id)
                .await
                .map_err(|err| err.to_string())?;
        }
        Ok(VideoEncodeOutcome::Skipped)
    }

    async fn cleanup_stale_encoded_files(
        &self,
        asset: &VideoConversionJob,
        output: &Path,
    ) -> Result<(), String> {
        let output_str = output.to_string_lossy();
        let mut paths = Vec::new();
        let mut ids = Vec::new();
        for file in &asset.files {
            if file.file_type == "encoded_video"
                && !file.is_edited
                && file.path != output_str
            {
                paths.push(file.path.clone());
                ids.push(file.id);
            }
        }
        if !paths.is_empty() {
            let _ = self
                .jobs
                .queue_file_delete(&paths)
                .await
                .map_err(|err| err.to_string());
            for id in ids {
                asset_job::delete_asset_file_by_id(&self.pool, &id)
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }
        Ok(())
    }

    async fn load_ffmpeg_config(&self) -> Result<FfmpegConfig, String> {
        let mut config = FfmpegConfig::default();
        let stored = get_json(&self.pool, "system-config")
            .await
            .map_err(|err| err.to_string())?;
        let Some(ffmpeg) = stored.and_then(|value| value.get("ffmpeg").cloned()) else {
            return Ok(config);
        };

        config.crf = read_u32(&ffmpeg, "crf", config.crf);
        config.threads = read_i32(&ffmpeg, "threads", config.threads);
        config.preset = read_string(&ffmpeg, "preset", &config.preset);
        config.target_video_codec = read_string(&ffmpeg, "targetVideoCodec", &config.target_video_codec);
        config.accepted_video_codecs =
            read_string_array(&ffmpeg, "acceptedVideoCodecs", &config.accepted_video_codecs);
        config.target_audio_codec = read_string(&ffmpeg, "targetAudioCodec", &config.target_audio_codec);
        config.accepted_audio_codecs =
            read_string_array(&ffmpeg, "acceptedAudioCodecs", &config.accepted_audio_codecs);
        config.accepted_containers =
            read_string_array(&ffmpeg, "acceptedContainers", &config.accepted_containers);
        config.target_resolution = read_string(&ffmpeg, "targetResolution", &config.target_resolution);
        config.max_bitrate = read_string(&ffmpeg, "maxBitrate", &config.max_bitrate);
        config.bframes = read_i32(&ffmpeg, "bframes", config.bframes);
        config.refs = read_u32(&ffmpeg, "refs", config.refs);
        config.gop_size = read_u32(&ffmpeg, "gopSize", config.gop_size);
        config.transcode = read_string(&ffmpeg, "transcode", &config.transcode);
        config.tonemap = read_string(&ffmpeg, "tonemap", &config.tonemap);
        config.accel = read_string(&ffmpeg, "accel", &config.accel);
        config.two_pass = ffmpeg
            .get("twoPass")
            .and_then(|v| v.as_bool())
            .unwrap_or(config.two_pass);

        Ok(config)
    }
}

fn find_encoded_video_file(files: &[VideoConversionFileRow]) -> Option<&VideoConversionFileRow> {
    files
        .iter()
        .find(|file| file.file_type == "encoded_video" && !file.is_edited)
}

fn get_transcode_target(config: &FfmpegConfig, asset: &VideoConversionJob) -> TranscodeTarget {
    let audio_required = is_audio_transcode_required(config, asset);
    let video_required = is_video_transcode_required(config, asset);

    match (audio_required, video_required) {
        (true, true) => TranscodeTarget::All,
        (true, false) => TranscodeTarget::Audio,
        (false, true) => TranscodeTarget::Video,
        (false, false) => TranscodeTarget::None,
    }
}

fn is_audio_transcode_required(config: &FfmpegConfig, asset: &VideoConversionJob) -> bool {
    let Some(codec) = asset.audio_codec_name.as_deref() else {
        return false;
    };

    match config.transcode.as_str() {
        "disabled" => false,
        "all" => true,
        "required" | "optimal" | "bitrate" => {
            !config
                .accepted_audio_codecs
                .iter()
                .any(|accepted| accepted.eq_ignore_ascii_case(codec))
        }
        _ => false,
    }
}

fn is_video_transcode_required(config: &FfmpegConfig, asset: &VideoConversionJob) -> bool {
    let scaling_enabled = config.target_resolution != "original";
    let target_res = config
        .target_resolution
        .parse::<i32>()
        .unwrap_or(720);
    let is_larger_than_target = scaling_enabled
        && asset.video_width.min(asset.video_height) > target_res;
    let max_bitrate = parse_bitrate_to_bps(&config.max_bitrate);
    let is_larger_than_target_bitrate = max_bitrate > 0 && asset.video_bitrate > max_bitrate;

    let is_target_codec = config
        .accepted_video_codecs
        .iter()
        .any(|accepted| accepted.eq_ignore_ascii_case(&asset.video_codec_name));
    let is_required = !is_target_codec || !asset.pixel_format.ends_with("420p");

    match config.transcode.as_str() {
        "disabled" => false,
        "all" => true,
        "required" => is_required,
        "optimal" => is_required || is_larger_than_target,
        "bitrate" => is_required || is_larger_than_target_bitrate,
        _ => false,
    }
}

fn is_remux_required(config: &FfmpegConfig, asset: &VideoConversionJob) -> bool {
    if config.transcode == "disabled" {
        return false;
    }

    let container = normalized_container_name(&asset.format_name, asset.format_long_name.as_deref());
    container != "mp4"
        && !config
            .accepted_containers
            .iter()
            .any(|accepted| accepted.eq_ignore_ascii_case(&container))
}

fn normalized_container_name(format_name: &str, format_long_name: Option<&str>) -> String {
    if let Some(long_name) = format_long_name {
        match long_name {
            "QuickTime / MOV" => return "mov".into(),
            "Matroska / WebM" => return "webm".into(),
            _ => {}
        }
    }
    format_name.to_ascii_lowercase()
}

fn parse_bitrate_to_bps(bitrate: &str) -> i64 {
    let trimmed = bitrate.trim();
    if trimmed.is_empty() {
        return 0;
    }
    let numeric: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    let value = numeric.parse::<i64>().unwrap_or(0);
    if value <= 0 {
        return 0;
    }
    if trimmed.ends_with('M') || trimmed.ends_with('m') {
        value * 1_000_000
    } else if trimmed.ends_with('K') || trimmed.ends_with('k') {
        value * 1_000
    } else {
        value * 1_000
    }
}

fn build_ffmpeg_args(
    input: &str,
    output: &Path,
    target: TranscodeTarget,
    remux_required: bool,
    config: &FfmpegConfig,
    asset: &VideoConversionJob,
) -> Result<Vec<String>, String> {
    let effective_target = if target == TranscodeTarget::None && remux_required {
        TranscodeTarget::None
    } else {
        target
    };

    let video_codec = match effective_target {
        TranscodeTarget::All | TranscodeTarget::Video => ffmpeg_video_encoder(&config.target_video_codec)
            .ok_or_else(|| format!("unsupported target video codec: {}", config.target_video_codec))?,
        _ => "copy",
    };

    let audio_codec = match effective_target {
        TranscodeTarget::All | TranscodeTarget::Audio => ffmpeg_audio_encoder(&config.target_audio_codec)
            .ok_or_else(|| format!("unsupported target audio codec: {}", config.target_audio_codec))?,
        _ => "copy",
    };

    let mut args = vec![
        "ffmpeg".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.into(),
        "-c:v".into(),
        video_codec.into(),
        "-c:a".into(),
        audio_codec.into(),
        "-map".into(),
        format!("0:{}", asset.video_index),
        "-map_metadata".into(),
        "-1".into(),
    ];

    if let Some(audio_index) = asset.audio_index {
        args.push("-map".into());
        args.push(format!("0:{audio_index}"));
    }

    if config.bframes > -1 {
        args.push("-bf".into());
        args.push(config.bframes.to_string());
    }
    if config.refs > 0 {
        args.push("-refs".into());
        args.push(config.refs.to_string());
    }
    if config.gop_size > 0 {
        args.push("-g".into());
        args.push(config.gop_size.to_string());
    }

    if matches!(effective_target, TranscodeTarget::All | TranscodeTarget::Video) {
        if config.threads > 0 {
            args.push("-threads".into());
            args.push(config.threads.to_string());
        }
        args.push("-preset".into());
        args.push(config.preset.clone());

        let max_bitrate = parse_bitrate_to_bps(&config.max_bitrate);
        if max_bitrate > 0 {
            let unit = bitrate_unit(&config.max_bitrate);
            let max_k = max_bitrate / if unit == "M" { 1_000_000 } else { 1_000 };
            args.push("-crf".into());
            args.push(config.crf.to_string());
            args.push("-maxrate".into());
            args.push(format!("{max_k}{unit}"));
            args.push("-bufsize".into());
            args.push(format!("{}{}", max_k * 2, unit));
        } else {
            args.push("-crf".into());
            args.push(config.crf.to_string());
        }

        if let Some(filter) = build_video_filter(config, asset) {
            args.push("-vf".into());
            args.push(filter);
        }
    }

    if config.target_video_codec.eq_ignore_ascii_case("hevc")
        && matches!(effective_target, TranscodeTarget::All | TranscodeTarget::Video)
    {
        args.push("-tag:v".into());
        args.push("hvc1".into());
    }

    args.push("-movflags".into());
    args.push("faststart".into());
    args.push("-fps_mode".into());
    args.push("passthrough".into());
    args.push(output.to_string_lossy().into_owned());

    Ok(args)
}

fn build_video_filter(config: &FfmpegConfig, asset: &VideoConversionJob) -> Option<String> {
    let mut filters = Vec::new();

    if should_scale(config, asset) {
        filters.push(scale_filter(config, asset));
    }

    if should_tone_map(config, asset) {
        filters.push(format!(
            "tonemapx=tonemap={}:desat=0:p=bt709:t=bt709:m=bt709:r=pc:peak=100:format=yuv420p",
            config.tonemap
        ));
    } else if !asset.pixel_format.ends_with("420p") {
        filters.push("format=yuv420p".into());
    }

    if filters.is_empty() {
        None
    } else {
        Some(filters.join(","))
    }
}

fn should_scale(config: &FfmpegConfig, asset: &VideoConversionJob) -> bool {
    let odd = asset.video_width % 2 != 0 || asset.video_height % 2 != 0;
    let target = target_resolution(config, asset);
    let larger = asset.video_width.min(asset.video_height) > target;
    odd || larger
}

fn target_resolution(config: &FfmpegConfig, asset: &VideoConversionJob) -> i32 {
    let mut target = if config.target_resolution == "original" {
        asset.video_width.min(asset.video_height)
    } else {
        config.target_resolution.parse::<i32>().unwrap_or(720)
    };
    if target % 2 != 0 {
        target -= 1;
    }
    target.max(2)
}

fn scale_filter(config: &FfmpegConfig, asset: &VideoConversionJob) -> String {
    let target = target_resolution(config, asset);
    if asset.video_height > asset.video_width {
        format!("{target}:-2")
    } else {
        format!("-2:{target}")
    }
}

fn should_tone_map(config: &FfmpegConfig, asset: &VideoConversionJob) -> bool {
    config.tonemap != "disabled"
        && asset.color_transfer.as_deref().is_some_and(|transfer| {
            transfer == "smpte2084" || transfer == "arib-std-b67"
        })
}

fn ffmpeg_video_encoder(codec: &str) -> Option<&'static str> {
    match codec.to_ascii_lowercase().as_str() {
        "h264" => Some("libx264"),
        "hevc" | "h265" => Some("libx265"),
        "vp9" => Some("libvpx-vp9"),
        "av1" => Some("libsvtav1"),
        _ => None,
    }
}

fn ffmpeg_audio_encoder(codec: &str) -> Option<&'static str> {
    match codec.to_ascii_lowercase().as_str() {
        "aac" => Some("aac"),
        "mp3" => Some("mp3"),
        "opus" => Some("libopus"),
        "pcm_s16le" => Some("pcm_s16le"),
        _ => None,
    }
}

fn bitrate_unit(max_bitrate: &str) -> &'static str {
    let trimmed = max_bitrate.trim();
    if trimmed.ends_with('M') || trimmed.ends_with('m') {
        "M"
    } else {
        "k"
    }
}

async fn run_ffmpeg(args: &[String]) -> Result<(), String> {
    let mut command = Command::new(&args[0]);
    command
        .args(&args[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let output = command
        .output()
        .await
        .map_err(|err| format!("failed to run ffmpeg: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg failed: {stderr}"));
    }
    Ok(())
}

fn read_string(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

fn read_string_array(value: &Value, key: &str, default: &[String]) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_else(|| default.to_vec())
}

fn read_u32(value: &Value, key: &str, default: u32) -> u32 {
    value
        .get(key)
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(default)
}

fn read_i32(value: &Value, key: &str, default: i32) -> i32 {
    value
        .get(key)
        .and_then(|v| v.as_i64())
        .and_then(|v| i32::try_from(v).ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_asset() -> VideoConversionJob {
        VideoConversionJob {
            id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            original_path: "/tmp/video.mp4".into(),
            video_index: 0,
            video_codec_name: "hevc".into(),
            video_bitrate: 10_000_000,
            video_width: 3840,
            video_height: 2160,
            pixel_format: "yuv420p".into(),
            frame_count: 1000,
            frame_rate: Some(30.0),
            rotation: 0,
            color_transfer: None,
            format_name: "mp4".into(),
            format_long_name: None,
            audio_index: Some(1),
            audio_codec_name: Some("aac".into()),
            files: Vec::new(),
        }
    }

    #[test]
    fn required_policy_transcodes_non_h264() {
        let config = FfmpegConfig::default();
        let asset = sample_asset();
        assert_eq!(get_transcode_target(&config, &asset), TranscodeTarget::Video);
    }

    #[test]
    fn compatible_h264_in_mp4_can_skip() {
        let config = FfmpegConfig::default();
        let mut asset = sample_asset();
        asset.video_codec_name = "h264".into();
        asset.video_width = 1280;
        asset.video_height = 720;
        assert_eq!(get_transcode_target(&config, &asset), TranscodeTarget::None);
        assert!(!is_remux_required(&config, &asset));
    }
}
