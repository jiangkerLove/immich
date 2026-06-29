use std::path::Path;

use serde_json::Value;

use crate::models::db::video_stream::VideoStreamAssetRow;
use crate::service::media::ffmpeg_tonemap::{
    should_tone_map_i32, tonemap_cuda_filter, tonemap_opencl_qsv_chain, tonemap_opencl_vaapi_chain,
    tonemapx_filter,
};
use crate::utils::hls::{hls_crf, HLS_SEGMENT_DURATION, HlsVariant};
use crate::utils::system_config::{json_bool, json_i32, json_str};
use crate::utils::video_interfaces::{resolve_hw_device, VideoInterfaces};

#[derive(Debug, Clone)]
pub struct HlsFfmpegSettings {
    pub accel: String,
    pub preferred_hw_device: String,
    pub preset: String,
    pub crf: u32,
    pub tonemap: String,
    pub threads: i32,
    pub accel_decode: bool,
}

impl HlsFfmpegSettings {
    pub fn from_config(config: &Value) -> Self {
        let ffmpeg = config.get("ffmpeg").cloned().unwrap_or_default();
        Self {
            accel: json_str(&ffmpeg, &["accel"], "disabled"),
            preferred_hw_device: json_str(&ffmpeg, &["preferredHwDevice"], "auto"),
            preset: json_str(&ffmpeg, &["preset"], "ultrafast"),
            crf: json_i32(&ffmpeg, &["crf"], 23).max(0) as u32,
            tonemap: json_str(&ffmpeg, &["tonemap"], "hable"),
            threads: json_i32(&ffmpeg, &["threads"], 0),
            accel_decode: json_bool(&ffmpeg, &["accelDecode"], true),
        }
    }
}

pub fn build_hls_ffmpeg_args(
    settings: &HlsFfmpegSettings,
    interfaces: &VideoInterfaces,
    asset: &VideoStreamAssetRow,
    variant: &HlsVariant,
    input_path: &str,
    variant_dir: &Path,
    start_segment: i32,
    gop: i32,
    seek_seconds: f64,
) -> Result<Vec<String>, String> {
    match settings.accel.as_str() {
        "nvenc" => build_nvenc_hls_args(
            settings,
            asset,
            variant,
            input_path,
            variant_dir,
            start_segment,
            gop,
            seek_seconds,
        ),
        "vaapi" => build_vaapi_hls_args(
            settings,
            interfaces,
            asset,
            variant,
            input_path,
            variant_dir,
            start_segment,
            gop,
            seek_seconds,
        ),
        "qsv" => build_qsv_hls_args(
            settings,
            interfaces,
            asset,
            variant,
            input_path,
            variant_dir,
            start_segment,
            gop,
            seek_seconds,
        ),
        "rkmpp" => build_rkmpp_hls_args(
            settings,
            interfaces,
            asset,
            variant,
            input_path,
            variant_dir,
            start_segment,
            gop,
            seek_seconds,
        ),
        "v4l2m2m" => build_v4l2m2m_hls_args(
            settings,
            asset,
            variant,
            input_path,
            variant_dir,
            start_segment,
            gop,
            seek_seconds,
        ),
        _ => Ok(build_software_hls_args(
            settings,
            asset,
            variant,
            input_path,
            variant_dir,
            start_segment,
            gop,
            seek_seconds,
        )),
    }
}

fn build_software_hls_args(
    settings: &HlsFfmpegSettings,
    asset: &VideoStreamAssetRow,
    variant: &HlsVariant,
    input_path: &str,
    variant_dir: &Path,
    start_segment: i32,
    gop: i32,
    seek_seconds: f64,
) -> Vec<String> {
    let video_encoder = software_encoder(variant.codec);
    let mut args = base_hls_args(seek_seconds, input_path);
    append_output_maps(&mut args, asset, video_encoder, "aac");
    append_gop(&mut args, gop);
    args.extend([
        "-preset".into(),
        settings.preset.clone(),
        "-crf".into(),
        hls_crf(variant.codec).to_string(),
        "-maxrate".into(),
        format!("{}k", variant.bitrate / 1000),
        "-bufsize".into(),
        format!("{}k", variant.bitrate / 500),
    ]);
    append_vf(&mut args, build_cpu_vf(settings, asset, variant.resolution));
    append_hls_muxer(
        &mut args,
        asset,
        variant_dir,
        start_segment,
        variant.codec == "hevc",
    );
    args
}

fn build_nvenc_hls_args(
    settings: &HlsFfmpegSettings,
    asset: &VideoStreamAssetRow,
    variant: &HlsVariant,
    input_path: &str,
    variant_dir: &Path,
    start_segment: i32,
    gop: i32,
    seek_seconds: f64,
) -> Result<Vec<String>, String> {
    let encoder = nvenc_encoder(variant.codec);
    let mut args = base_hls_args(seek_seconds, input_path);
    if settings.accel_decode {
        args.extend([
            "-hwaccel".into(),
            "cuda".into(),
            "-hwaccel_output_format".into(),
            "cuda".into(),
            "-noautorotate".into(),
        ]);
    } else {
        args.extend([
            "-init_hw_device".into(),
            "cuda=cuda:0".into(),
            "-filter_hw_device".into(),
            "cuda".into(),
        ]);
    }

    append_output_maps(&mut args, asset, encoder, "aac");
    append_gop(&mut args, gop);
    append_nvenc_rate(&mut args, variant, hls_crf(variant.codec));
    args.extend([
        "-preset".into(),
        "p4".into(),
        "-tune".into(),
        "hq".into(),
        "-forced-idr".into(),
        "1".into(),
    ]);

    append_vf(
        &mut args,
        Some(build_nvenc_vf(
            settings,
            asset,
            variant.resolution,
            settings.accel_decode,
        )),
    );

    append_hls_muxer(
        &mut args,
        asset,
        variant_dir,
        start_segment,
        variant.codec == "hevc",
    );
    Ok(args)
}

fn build_vaapi_hls_args(
    settings: &HlsFfmpegSettings,
    interfaces: &VideoInterfaces,
    asset: &VideoStreamAssetRow,
    variant: &HlsVariant,
    input_path: &str,
    variant_dir: &Path,
    start_segment: i32,
    gop: i32,
    seek_seconds: f64,
) -> Result<Vec<String>, String> {
    let device = resolve_hw_device(interfaces, &settings.preferred_hw_device)?;
    let encoder = vaapi_encoder(variant.codec);
    let mut args = base_hls_args(seek_seconds, input_path);

    if settings.accel_decode {
        args.extend([
            "-hwaccel".into(),
            "vaapi".into(),
            "-hwaccel_output_format".into(),
            "vaapi".into(),
            "-noautorotate".into(),
            "-hwaccel_device".into(),
            device.clone(),
        ]);
    } else {
        args.extend([
            "-init_hw_device".into(),
            format!("vaapi=accel:{device}"),
            "-filter_hw_device".into(),
            "accel".into(),
        ]);
    }

    append_output_maps(&mut args, asset, encoder, "aac");
    append_gop_and_rate(&mut args, gop, variant, settings.preset.as_str(), hls_crf(variant.codec));
    args.extend(["-forced-idr".into(), "1".into()]);

    append_vf(
        &mut args,
        Some(build_vaapi_vf(
            settings,
            asset,
            variant.resolution,
            settings.accel_decode,
        )),
    );

    append_hls_muxer(
        &mut args,
        asset,
        variant_dir,
        start_segment,
        variant.codec == "hevc",
    );
    Ok(args)
}

fn build_qsv_hls_args(
    settings: &HlsFfmpegSettings,
    interfaces: &VideoInterfaces,
    asset: &VideoStreamAssetRow,
    variant: &HlsVariant,
    input_path: &str,
    variant_dir: &Path,
    start_segment: i32,
    gop: i32,
    seek_seconds: f64,
) -> Result<Vec<String>, String> {
    let device = resolve_hw_device(interfaces, &settings.preferred_hw_device)?;
    let encoder = qsv_encoder(variant.codec);
    let mut args = base_hls_args(seek_seconds, input_path);

    if settings.accel_decode {
        args.extend([
            "-hwaccel".into(),
            "qsv".into(),
            "-hwaccel_output_format".into(),
            "qsv".into(),
            "-async_depth".into(),
            "4".into(),
            "-noautorotate".into(),
            "-qsv_device".into(),
            device.clone(),
        ]);
    } else {
        args.extend([
            "-init_hw_device".into(),
            format!("qsv=hw,child_device={device}"),
            "-filter_hw_device".into(),
            "hw".into(),
        ]);
    }

    append_output_maps(&mut args, asset, encoder, "aac");
    append_gop_and_rate(&mut args, gop, variant, "4", hls_crf(variant.codec));
    args.extend(["-forced-idr".into(), "0".into()]);

    append_vf(
        &mut args,
        Some(build_qsv_vf(
            settings,
            asset,
            variant.resolution,
            settings.accel_decode,
        )),
    );

    append_hls_muxer(
        &mut args,
        asset,
        variant_dir,
        start_segment,
        variant.codec == "hevc",
    );
    Ok(args)
}

fn build_rkmpp_hls_args(
    settings: &HlsFfmpegSettings,
    interfaces: &VideoInterfaces,
    asset: &VideoStreamAssetRow,
    variant: &HlsVariant,
    input_path: &str,
    variant_dir: &Path,
    start_segment: i32,
    gop: i32,
    seek_seconds: f64,
) -> Result<Vec<String>, String> {
    if !matches!(variant.codec, "h264" | "hevc") {
        return Err(format!(
            "RKMPP acceleration does not support codec '{}'",
            variant.codec
        ));
    }

    let encoder = rkmpp_encoder(variant.codec);
    let mut args = base_hls_args(seek_seconds, input_path);
    if settings.accel_decode {
        args.extend([
            "-hwaccel".into(),
            "rkmpp".into(),
            "-hwaccel_output_format".into(),
            "drm_prime".into(),
            "-afbc".into(),
            "rga".into(),
            "-noautorotate".into(),
        ]);
    }

    append_output_maps(&mut args, asset, encoder, "aac");
    append_gop(&mut args, gop);
    append_rkmpp_level(&mut args, variant.codec);
    append_rkmpp_rate(&mut args, variant, settings.crf);
    args.extend(["-forced-idr".into(), "1".into()]);

    append_vf(
        &mut args,
        build_rkmpp_vf(
            settings,
            interfaces,
            asset,
            variant.resolution,
            settings.accel_decode,
        ),
    );

    append_hls_muxer(
        &mut args,
        asset,
        variant_dir,
        start_segment,
        variant.codec == "hevc",
    );
    Ok(args)
}

fn build_v4l2m2m_hls_args(
    settings: &HlsFfmpegSettings,
    asset: &VideoStreamAssetRow,
    variant: &HlsVariant,
    input_path: &str,
    variant_dir: &Path,
    start_segment: i32,
    gop: i32,
    seek_seconds: f64,
) -> Result<Vec<String>, String> {
    if !matches!(variant.codec, "h264" | "hevc") {
        return Err(format!(
            "V4L2M2M acceleration does not support codec '{}'",
            variant.codec
        ));
    }

    let encoder = v4l2m2m_encoder(variant.codec);
    let mut args = base_hls_args(seek_seconds, input_path);
    if settings.accel_decode {
        args.extend(["-hwaccel".into(), "v4l2m2m".into()]);
    }

    append_output_maps(&mut args, asset, encoder, "aac");
    append_gop(&mut args, gop);
    let max_k = variant.bitrate / 1000;
    args.extend([
        "-b:v".into(),
        format!("{max_k}k"),
        "-maxrate".into(),
        format!("{max_k}k"),
        "-bufsize".into(),
        format!("{}k", max_k * 2),
    ]);

    append_vf(&mut args, build_cpu_vf(settings, asset, variant.resolution));

    append_hls_muxer(
        &mut args,
        asset,
        variant_dir,
        start_segment,
        variant.codec == "hevc",
    );
    Ok(args)
}

fn build_cpu_vf(
    settings: &HlsFfmpegSettings,
    asset: &VideoStreamAssetRow,
    target_res: u32,
) -> Option<String> {
    let mut filters = Vec::new();
    if let Some(scale) = build_scale_filter(asset, target_res) {
        filters.push(scale);
    }
    if should_tone_map_i32(settings.tonemap.as_str(), asset.color_transfer) {
        filters.push(tonemapx_filter(&settings.tonemap));
    } else if !asset.pixel_format.ends_with("420p") {
        filters.push("format=yuv420p".into());
    }
    if filters.is_empty() {
        None
    } else {
        Some(filters.join(","))
    }
}

fn build_nvenc_vf(
    settings: &HlsFfmpegSettings,
    asset: &VideoStreamAssetRow,
    target_res: u32,
    hw_decode: bool,
) -> String {
    let scale = scale_expression(asset, target_res);
    let mut filters = Vec::new();

    if hw_decode {
        if should_scale(asset, target_res)
            || (!should_tone_map_i32(settings.tonemap.as_str(), asset.color_transfer) && !asset.pixel_format.ends_with("420p"))
        {
            filters.push(format!("scale_cuda={scale}"));
        }
        if should_tone_map_i32(settings.tonemap.as_str(), asset.color_transfer) {
            filters.push(tonemap_cuda_filter(&settings.tonemap));
        } else if !filters.is_empty() {
            let last = filters.len() - 1;
            if !filters[last].contains("format=") {
                filters[last] = format!("{}:format=nv12", filters[last]);
            }
        }
    } else {
        filters.push("hwupload_cuda".into());
        if should_scale(asset, target_res) {
            filters.push(format!("scale_cuda={scale}:format=nv12"));
        } else if should_tone_map_i32(settings.tonemap.as_str(), asset.color_transfer) {
            filters.push(tonemap_cuda_filter(&settings.tonemap));
        } else {
            filters.push(format!("scale_cuda={scale}:format=nv12"));
        }
    }

    filters.join(",")
}

fn build_vaapi_vf(
    settings: &HlsFfmpegSettings,
    asset: &VideoStreamAssetRow,
    target_res: u32,
    hw_decode: bool,
) -> String {
    let scale = scale_expression(asset, target_res);
    let mut filters = Vec::new();

    if hw_decode {
        if !should_tone_map_i32(settings.tonemap.as_str(), asset.color_transfer) && !asset.pixel_format.ends_with("420p") {
            filters.push(format!(
                "scale_vaapi={scale}:mode=hq:out_range=pc:format=nv12"
            ));
        } else if should_scale(asset, target_res) {
            filters.push(format!("scale_vaapi={scale}:mode=hq:out_range=pc"));
        }
        if should_tone_map_i32(settings.tonemap.as_str(), asset.color_transfer) {
            filters.push(tonemap_opencl_vaapi_chain(&settings.tonemap));
        }
    } else {
        if should_tone_map_i32(settings.tonemap.as_str(), asset.color_transfer) {
            filters.push(tonemapx_filter(&settings.tonemap));
        }
        filters.push("hwupload=extra_hw_frames=64".into());
        if should_scale(asset, target_res) {
            filters.push(format!(
                "scale_vaapi={scale}:mode=hq:out_range=pc:format=nv12"
            ));
        }
    }

    filters.join(",")
}

fn build_qsv_vf(
    settings: &HlsFfmpegSettings,
    asset: &VideoStreamAssetRow,
    target_res: u32,
    hw_decode: bool,
) -> String {
    let scale = scale_expression(asset, target_res);
    let mut filters = Vec::new();

    if hw_decode {
        if !should_tone_map_i32(settings.tonemap.as_str(), asset.color_transfer) && !asset.pixel_format.ends_with("420p") {
            filters.push(format!(
                "scale_qsv={scale}:async_depth=4:mode=hq:format=nv12"
            ));
        } else if should_scale(asset, target_res) {
            filters.push(format!("scale_qsv={scale}:async_depth=4:mode=hq"));
        }
        if should_tone_map_i32(settings.tonemap.as_str(), asset.color_transfer) {
            filters.push(tonemap_opencl_qsv_chain(&settings.tonemap));
        }
    } else {
        if should_tone_map_i32(settings.tonemap.as_str(), asset.color_transfer) {
            filters.push(tonemapx_filter(&settings.tonemap));
        }
        filters.push("hwupload=extra_hw_frames=64".into());
        if should_scale(asset, target_res) {
            filters.push(format!("scale_qsv={scale}:mode=hq:format=nv12"));
        }
    }

    filters.join(",")
}

fn build_rkmpp_vf(
    settings: &HlsFfmpegSettings,
    interfaces: &VideoInterfaces,
    asset: &VideoStreamAssetRow,
    target_res: u32,
    hw_decode: bool,
) -> Option<String> {
    if !hw_decode {
        return build_cpu_vf(settings, asset, target_res);
    }

    let scale = scale_expression(asset, target_res);
    if should_tone_map_i32(settings.tonemap.as_str(), asset.color_transfer) {
        if interfaces.mali {
            return Some(format!(
                "scale_rkrga={scale}:format=p010:afbc=1:async_depth=4,\
                 hwmap=derive_device=opencl:mode=read,\
                 tonemap_opencl=format=nv12:r=pc:p=bt709:t=bt709:m=bt709:tonemap={}:desat=0:tonemap_mode=lum:peak=100,\
                 hwmap=derive_device=rkmpp:mode=write:reverse=1,format=drm_prime",
                settings.tonemap
            ));
        }
        return Some(format!(
            "scale_rkrga={scale}:format=p010:afbc=1:async_depth=4,\
             hwdownload,format=p010,\
             tonemapx=tonemap={}:desat=0:p=bt709:t=bt709:m=bt709:r=pc:peak=100:format=yuv420p,\
             hwupload",
            settings.tonemap
        ));
    }

    if should_scale(asset, target_res) {
        return Some(format!(
            "scale_rkrga={scale}:format=nv12:afbc=1:async_depth=4"
        ));
    }

    None
}

fn append_vf(args: &mut Vec<String>, filter: Option<String>) {
    if let Some(filter) = filter.filter(|value| !value.is_empty()) {
        args.extend(["-vf".into(), filter]);
    }
}

fn base_hls_args(seek_seconds: f64, input_path: &str) -> Vec<String> {
    let mut args = vec![
        "ffmpeg".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostdin".into(),
        "-nostats".into(),
    ];
    if seek_seconds > 0.0 {
        args.push("-ss".into());
        args.push(format!("{seek_seconds}"));
    }
    args.extend(["-i".into(), input_path.into()]);
    args
}

fn append_output_maps(
    args: &mut Vec<String>,
    asset: &VideoStreamAssetRow,
    video_encoder: &str,
    audio_encoder: &str,
) {
    args.extend([
        "-c:v".into(),
        video_encoder.into(),
        "-c:a".into(),
        audio_encoder.into(),
        "-map".into(),
        format!("0:{}", asset.video_index),
        "-map_metadata".into(),
        "-1".into(),
    ]);
    if let Some(audio_index) = asset.audio_index {
        args.push("-map".into());
        args.push(format!("0:{audio_index}"));
    }
}

fn append_gop(args: &mut Vec<String>, gop: i32) {
    if gop > 0 {
        args.extend([
            "-g".into(),
            gop.to_string(),
            "-keyint_min".into(),
            gop.to_string(),
        ]);
    }
}

fn append_nvenc_rate(args: &mut Vec<String>, variant: &HlsVariant, crf: u32) {
    let max_k = variant.bitrate / 1000;
    args.extend([
        "-cq:v".into(),
        crf.to_string(),
        "-maxrate".into(),
        format!("{max_k}k"),
        "-bufsize".into(),
        format!("{}k", max_k * 2),
    ]);
}

fn append_rkmpp_level(args: &mut Vec<String>, codec: &str) {
    let level = if codec == "hevc" { "153" } else { "51" };
    args.extend(["-level".into(), level.into()]);
}

fn append_rkmpp_rate(args: &mut Vec<String>, variant: &HlsVariant, crf: u32) {
    let max_k = variant.bitrate / 1000;
    if max_k > 0 {
        args.extend([
            "-rc_mode".into(),
            "AVBR".into(),
            "-b:v".into(),
            format!("{max_k}k"),
        ]);
    } else {
        args.extend([
            "-rc_mode".into(),
            "CQP".into(),
            "-qp_init".into(),
            crf.to_string(),
        ]);
    }
}

fn rkmpp_encoder(codec: &str) -> &'static str {
    match codec {
        "hevc" => "hevc_rkmpp",
        _ => "h264_rkmpp",
    }
}

fn v4l2m2m_encoder(codec: &str) -> &'static str {
    match codec {
        "hevc" => "hevc_v4l2m2m",
        _ => "h264_v4l2m2m",
    }
}

fn should_scale(asset: &VideoStreamAssetRow, target_res: u32) -> bool {
    let min_dim = asset.width.min(asset.height);
    let odd = asset.width % 2 != 0 || asset.height % 2 != 0;
    min_dim > target_res as i32 || odd
}

fn append_gop_and_rate(
    args: &mut Vec<String>,
    gop: i32,
    variant: &HlsVariant,
    preset: &str,
    crf: u32,
) {
    if gop > 0 {
        args.extend([
            "-g".into(),
            gop.to_string(),
            "-keyint_min".into(),
            gop.to_string(),
        ]);
    }
    args.extend([
        "-preset".into(),
        preset.into(),
        "-crf".into(),
        crf.to_string(),
        "-maxrate".into(),
        format!("{}k", variant.bitrate / 1000),
        "-bufsize".into(),
        format!("{}k", variant.bitrate / 500),
    ]);
}

fn append_hls_muxer(
    args: &mut Vec<String>,
    asset: &VideoStreamAssetRow,
    variant_dir: &Path,
    start_segment: i32,
    hevc_tag: bool,
) {
    if hevc_tag {
        args.extend(["-tag:v".into(), "hvc1".into()]);
    }
    args.extend([
        "-copyts".into(),
        "-r".into(),
        format!("{}/{}", asset.packet_count, asset.total_duration),
        "-avoid_negative_ts".into(),
        "disabled".into(),
        "-f".into(),
        "hls".into(),
        "-hls_time".into(),
        format!("{HLS_SEGMENT_DURATION}"),
        "-hls_list_size".into(),
        "0".into(),
        "-hls_segment_type".into(),
        "fmp4".into(),
        "-hls_fmp4_init_filename".into(),
        "init.mp4".into(),
        "-hls_segment_options".into(),
        "movflags=+frag_discont".into(),
        "-hls_flags".into(),
        "temp_file".into(),
        "-hls_segment_filename".into(),
        variant_dir.join("seg_%d.m4s").to_string_lossy().into_owned(),
        "-start_number".into(),
        start_segment.to_string(),
        variant_dir.join("playlist.m3u8").to_string_lossy().into_owned(),
    ]);
}

fn software_encoder(codec: &str) -> &'static str {
    match codec {
        "hevc" => "libx265",
        "av1" => "libsvtav1",
        _ => "libx264",
    }
}

fn nvenc_encoder(codec: &str) -> &'static str {
    match codec {
        "hevc" => "hevc_nvenc",
        "av1" => "av1_nvenc",
        _ => "h264_nvenc",
    }
}

fn vaapi_encoder(codec: &str) -> &'static str {
    match codec {
        "hevc" => "hevc_vaapi",
        "av1" => "av1_vaapi",
        _ => "h264_vaapi",
    }
}

fn qsv_encoder(codec: &str) -> &'static str {
    match codec {
        "hevc" => "hevc_qsv",
        "av1" => "av1_qsv",
        _ => "h264_qsv",
    }
}

fn scale_expression(asset: &VideoStreamAssetRow, target_res: u32) -> String {
    let (width, height) = crate::models::db::video_stream::output_size(
        asset.width,
        asset.height,
        asset.orientation,
        target_res,
    );
    format!("{width}:{height}")
}

fn build_scale_filter(asset: &VideoStreamAssetRow, target_res: u32) -> Option<String> {
    if !should_scale(asset, target_res) {
        return None;
    }
    Some(format!("scale={}", scale_expression(asset, target_res)))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use uuid::Uuid;

    use super::*;
    use crate::service::media::ffmpeg_tonemap::{
        should_tone_map_i32, COLOR_TRANSFER_ARIB_STD_B67, COLOR_TRANSFER_SMPTE2084,
    };
    use crate::utils::hls::HLS_VARIANTS;

    fn test_asset(color_transfer: i32) -> VideoStreamAssetRow {
        VideoStreamAssetRow {
            original_path: "/tmp/video.mp4".into(),
            owner_id: Uuid::new_v4(),
            video_index: 0,
            codec_name: "hevc".into(),
            width: 3840,
            height: 2160,
            time_base: 1,
            frame_count: 100,
            frame_rate: Some(30.0),
            orientation: None,
            pixel_format: "yuv420p10le".into(),
            color_transfer,
            packet_count: 3000,
            output_frames: 100,
            total_duration: 100_000,
            audio_index: Some(1),
        }
    }

    fn test_settings(tonemap: &str, accel: &str) -> HlsFfmpegSettings {
        HlsFfmpegSettings {
            accel: accel.into(),
            preferred_hw_device: "auto".into(),
            preset: "ultrafast".into(),
            crf: 23,
            tonemap: tonemap.into(),
            threads: 0,
            accel_decode: true,
        }
    }

    #[test]
    fn tone_map_requires_hdr_and_enabled_tonemap() {
        let settings = test_settings("hable", "disabled");
        let hdr = test_asset(COLOR_TRANSFER_SMPTE2084);
        let sdr = test_asset(1);

        assert!(should_tone_map_i32("hable", hdr.color_transfer));
        assert!(!should_tone_map_i32("hable", sdr.color_transfer));
        assert!(!should_tone_map_i32("disabled", hdr.color_transfer));
    }

    #[test]
    fn software_hls_includes_tonemapx_for_hdr() {
        let settings = test_settings("hable", "disabled");
        let asset = test_asset(COLOR_TRANSFER_SMPTE2084);
        let variant = &HLS_VARIANTS[2];
        let args = build_software_hls_args(
            &settings,
            &asset,
            variant,
            "/tmp/video.mp4",
            Path::new("/tmp/hls"),
            0,
            60,
            0.0,
        );
        let joined = args.join(" ");
        assert!(joined.contains("tonemapx=tonemap=hable"));
        assert!(joined.contains("libx264"));
    }

    #[test]
    fn nvenc_hls_includes_tonemap_cuda_for_hdr_hw_decode() {
        let settings = test_settings("hable", "nvenc");
        let asset = test_asset(COLOR_TRANSFER_ARIB_STD_B67);
        let variant = &HLS_VARIANTS[2];
        let args = build_nvenc_hls_args(
            &settings,
            &asset,
            variant,
            "/tmp/video.mp4",
            Path::new("/tmp/hls"),
            0,
            60,
            0.0,
        )
        .expect("nvenc args");
        let joined = args.join(" ");
        assert!(joined.contains("tonemap_cuda=desat=0"));
        assert!(joined.contains("h264_nvenc"));
        assert!(joined.contains("-cq:v"));
    }

    #[test]
    fn v4l2m2m_uses_hardware_encoder() {
        let settings = test_settings("hable", "v4l2m2m");
        let asset = test_asset(COLOR_TRANSFER_SMPTE2084);
        let variant = &HLS_VARIANTS[1];
        let args = build_v4l2m2m_hls_args(
            &settings,
            &asset,
            variant,
            "/tmp/video.mp4",
            Path::new("/tmp/hls"),
            0,
            60,
            0.0,
        )
        .expect("v4l2m2m args");
        let joined = args.join(" ");
        assert!(joined.contains("hevc_v4l2m2m"));
        assert!(joined.contains("-hwaccel"));
        assert!(joined.contains("v4l2m2m"));
        assert!(joined.contains("tonemapx=tonemap=hable"));
    }

    #[test]
    fn rkmpp_hdr_with_mali_uses_opencl_tonemap() {
        let settings = test_settings("mobius", "rkmpp");
        let asset = test_asset(COLOR_TRANSFER_SMPTE2084);
        let variant = &HLS_VARIANTS[1];
        let interfaces = VideoInterfaces {
            dri: vec!["renderD128".into()],
            mali: true,
        };
        let vf = build_rkmpp_vf(&settings, &interfaces, &asset, variant.resolution, true)
            .expect("rkmpp vf");
        assert!(vf.contains("tonemap_opencl"));
        assert!(vf.contains("tonemap=mobius"));
    }
}
