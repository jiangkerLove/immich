use crate::models::db::asset_job::VideoConversionJob;
use crate::service::media::ffmpeg_tonemap::{
    should_tone_map_i16, tonemap_cuda_filter, tonemap_opencl_qsv_chain, tonemap_opencl_vaapi_chain,
    tonemapx_filter,
};
use crate::utils::hls::supported_codecs_for_accel;
use crate::utils::video_interfaces::{resolve_hw_device, VideoInterfaces};

#[derive(Debug, Clone)]
pub struct VideoHwConfig<'a> {
    pub accel: &'a str,
    pub accel_decode: bool,
    pub preferred_hw_device: &'a str,
    pub tonemap: &'a str,
    pub target_video_codec: &'a str,
    pub preset: &'a str,
    pub crf: u32,
    pub max_bitrate: &'a str,
}

pub fn hw_video_encoder(accel: &str, codec: &str) -> Option<&'static str> {
    if !supported_codecs_for_accel(accel)
        .iter()
        .any(|supported| supported.eq_ignore_ascii_case(codec))
    {
        return None;
    }

    match accel {
        "nvenc" => match codec.to_ascii_lowercase().as_str() {
            "hevc" | "h265" => Some("hevc_nvenc"),
            "av1" => Some("av1_nvenc"),
            _ => Some("h264_nvenc"),
        },
        "vaapi" => match codec.to_ascii_lowercase().as_str() {
            "hevc" | "h265" => Some("hevc_vaapi"),
            "av1" => Some("av1_vaapi"),
            "vp9" => Some("vp9_vaapi"),
            _ => Some("h264_vaapi"),
        },
        "qsv" => match codec.to_ascii_lowercase().as_str() {
            "hevc" | "h265" => Some("hevc_qsv"),
            "av1" => Some("av1_qsv"),
            "vp9" => Some("vp9_qsv"),
            _ => Some("h264_qsv"),
        },
        "rkmpp" => match codec.to_ascii_lowercase().as_str() {
            "hevc" | "h265" => Some("hevc_rkmpp"),
            "h264" => Some("h264_rkmpp"),
            _ => None,
        },
        "v4l2m2m" => match codec.to_ascii_lowercase().as_str() {
            "hevc" | "h265" => Some("hevc_v4l2m2m"),
            "h264" => Some("h264_v4l2m2m"),
            _ => None,
        },
        _ => None,
    }
}

pub fn append_hw_input_options(
    args: &mut Vec<String>,
    config: &VideoHwConfig<'_>,
    interfaces: &VideoInterfaces,
) -> Result<(), String> {
    match config.accel {
        "nvenc" if config.accel_decode => {
            args.extend([
                "-hwaccel".into(),
                "cuda".into(),
                "-hwaccel_output_format".into(),
                "cuda".into(),
                "-noautorotate".into(),
            ]);
        }
        "nvenc" => {
            args.extend([
                "-init_hw_device".into(),
                "cuda=cuda:0".into(),
                "-filter_hw_device".into(),
                "cuda".into(),
            ]);
        }
        "vaapi" if config.accel_decode => {
            let device = resolve_hw_device(interfaces, config.preferred_hw_device)?;
            args.extend([
                "-hwaccel".into(),
                "vaapi".into(),
                "-hwaccel_output_format".into(),
                "vaapi".into(),
                "-noautorotate".into(),
                "-hwaccel_device".into(),
                device,
            ]);
        }
        "vaapi" => {
            let device = resolve_hw_device(interfaces, config.preferred_hw_device)?;
            args.extend([
                "-init_hw_device".into(),
                format!("vaapi=accel:{device}"),
                "-filter_hw_device".into(),
                "accel".into(),
            ]);
        }
        "qsv" if config.accel_decode => {
            let device = resolve_hw_device(interfaces, config.preferred_hw_device)?;
            args.extend([
                "-hwaccel".into(),
                "qsv".into(),
                "-hwaccel_output_format".into(),
                "qsv".into(),
                "-async_depth".into(),
                "4".into(),
                "-noautorotate".into(),
                "-qsv_device".into(),
                device,
            ]);
        }
        "qsv" => {
            let device = resolve_hw_device(interfaces, config.preferred_hw_device)?;
            args.extend([
                "-init_hw_device".into(),
                format!("qsv=hw,child_device={device}"),
                "-filter_hw_device".into(),
                "hw".into(),
            ]);
        }
        "rkmpp" if config.accel_decode => {
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
        "v4l2m2m" if config.accel_decode => {
            args.extend(["-hwaccel".into(), "v4l2m2m".into()]);
        }
        _ => {}
    }
    Ok(())
}

pub fn build_hw_video_filter(
    config: &VideoHwConfig<'_>,
    interfaces: &VideoInterfaces,
    asset: &VideoConversionJob,
    target_res: i32,
) -> Option<String> {
    let chain = match config.accel {
        "nvenc" => build_nvenc_vf(config, asset, target_res, config.accel_decode),
        "vaapi" => build_vaapi_vf(config, asset, target_res, config.accel_decode),
        "qsv" => build_qsv_vf(config, asset, target_res, config.accel_decode),
        "rkmpp" => {
            return build_rkmpp_vf(config, interfaces, asset, target_res, config.accel_decode)
        }
        "v4l2m2m" => {
            return build_cpu_vf(config, asset, target_res)
        }
        _ => return None,
    };
    if chain.is_empty() { None } else { Some(chain) }
}

pub fn append_hw_rate_options(args: &mut Vec<String>, config: &VideoHwConfig<'_>) {
    let max_bps = parse_bitrate_to_bps(config.max_bitrate);
    let max_k = max_bps / 1_000;

    match config.accel {
        "nvenc" => {
            if max_k > 0 {
                args.extend([
                    "-cq:v".into(),
                    config.crf.to_string(),
                    "-maxrate".into(),
                    format!("{max_k}k"),
                    "-bufsize".into(),
                    format!("{}k", max_k * 2),
                ]);
            } else {
                args.extend(["-cq:v".into(), config.crf.to_string()]);
            }
            args.extend([
                "-preset".into(),
                nvenc_preset(config.preset),
                "-tune".into(),
                "hq".into(),
            ]);
        }
        "rkmpp" => {
            append_rkmpp_level(args, config.target_video_codec);
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
                    config.crf.to_string(),
                ]);
            }
        }
        "v4l2m2m" if max_k > 0 => {
            args.extend([
                "-b:v".into(),
                format!("{max_k}k"),
                "-maxrate".into(),
                format!("{max_k}k"),
                "-bufsize".into(),
                format!("{}k", max_k * 2),
            ]);
        }
        "vaapi" | "qsv" => {
            if max_k > 0 {
                args.extend([
                    "-maxrate".into(),
                    format!("{max_k}k"),
                    "-bufsize".into(),
                    format!("{}k", max_k * 2),
                ]);
            }
            args.extend([
                "-global_quality:v".into(),
                config.crf.to_string(),
            ]);
            if config.accel == "qsv" {
                args.extend(["-preset".into(), qsv_preset(config.preset)]);
            } else {
                args.extend([
                    "-compression_level".into(),
                    vaapi_compression_level(config.preset),
                ]);
            }
        }
        _ => {}
    }
}

fn build_nvenc_vf(
    config: &VideoHwConfig<'_>,
    asset: &VideoConversionJob,
    target_res: i32,
    hw_decode: bool,
) -> String {
    let scale = scale_expression(asset, target_res);
    let tone_map = should_tone_map_i16(config.tonemap, asset.color_transfer);
    let mut filters = Vec::new();
    if hw_decode {
        if should_scale(asset, target_res)
            || (!tone_map && !asset.pixel_format.ends_with("420p"))
        {
            filters.push(format!("scale_cuda={scale}"));
        }
        if tone_map {
            filters.push(tonemap_cuda_filter(config.tonemap));
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
        } else if tone_map {
            filters.push(tonemap_cuda_filter(config.tonemap));
        } else {
            filters.push(format!("scale_cuda={scale}:format=nv12"));
        }
    }
    filters.join(",")
}

fn build_vaapi_vf(
    config: &VideoHwConfig<'_>,
    asset: &VideoConversionJob,
    target_res: i32,
    hw_decode: bool,
) -> String {
    let scale = scale_expression(asset, target_res);
    let tone_map = should_tone_map_i16(config.tonemap, asset.color_transfer);
    let mut filters = Vec::new();
    if hw_decode {
        if !tone_map && !asset.pixel_format.ends_with("420p") {
            filters.push(format!("scale_vaapi={scale}:mode=hq:out_range=pc:format=nv12"));
        } else if should_scale(asset, target_res) {
            filters.push(format!("scale_vaapi={scale}:mode=hq:out_range=pc"));
        }
        if tone_map {
            filters.push(tonemap_opencl_vaapi_chain(config.tonemap));
        }
    } else {
        if tone_map {
            filters.push(tonemapx_filter(config.tonemap));
        }
        filters.push("hwupload=extra_hw_frames=64".into());
        if should_scale(asset, target_res) {
            filters.push(format!("scale_vaapi={scale}:mode=hq:out_range=pc:format=nv12"));
        }
    }
    filters.join(",")
}

fn build_qsv_vf(
    config: &VideoHwConfig<'_>,
    asset: &VideoConversionJob,
    target_res: i32,
    hw_decode: bool,
) -> String {
    let scale = scale_expression(asset, target_res);
    let tone_map = should_tone_map_i16(config.tonemap, asset.color_transfer);
    let mut filters = Vec::new();
    if hw_decode {
        if !tone_map && !asset.pixel_format.ends_with("420p") {
            filters.push(format!("scale_qsv={scale}:async_depth=4:mode=hq:format=nv12"));
        } else if should_scale(asset, target_res) {
            filters.push(format!("scale_qsv={scale}:async_depth=4:mode=hq"));
        }
        if tone_map {
            filters.push(tonemap_opencl_qsv_chain(config.tonemap));
        }
    } else {
        if tone_map {
            filters.push(tonemapx_filter(config.tonemap));
        }
        filters.push("hwupload=extra_hw_frames=64".into());
        if should_scale(asset, target_res) {
            filters.push(format!("scale_qsv={scale}:mode=hq:format=nv12"));
        }
    }
    filters.join(",")
}

fn build_rkmpp_vf(
    config: &VideoHwConfig<'_>,
    interfaces: &VideoInterfaces,
    asset: &VideoConversionJob,
    target_res: i32,
    hw_decode: bool,
) -> Option<String> {
    let scale = scale_expression(asset, target_res);
    let tone_map = should_tone_map_i16(config.tonemap, asset.color_transfer);

    if !hw_decode {
        return build_cpu_vf(config, asset, target_res);
    }

    if tone_map {
        if interfaces.mali {
            return Some(format!(
                "scale_rkrga={scale}:format=p010:afbc=1:async_depth=4,\
                 hwmap=derive_device=opencl:mode=read,\
                 tonemap_opencl=format=nv12:r=pc:p=bt709:t=bt709:m=bt709:tonemap={}:desat=0:tonemap_mode=lum:peak=100,\
                 hwmap=derive_device=rkmpp:mode=write:reverse=1,format=drm_prime",
                config.tonemap
            ));
        }
        return Some(format!(
            "scale_rkrga={scale}:format=p010:afbc=1:async_depth=4,\
             hwdownload,format=p010,\
             tonemapx=tonemap={}:desat=0:p=bt709:t=bt709:m=bt709:r=pc:peak=100:format=yuv420p,\
             hwupload",
            config.tonemap
        ));
    }

    if should_scale(asset, target_res) {
        return Some(format!(
            "scale_rkrga={scale}:format=nv12:afbc=1:async_depth=4"
        ));
    }

    None
}

fn build_cpu_vf(
    config: &VideoHwConfig<'_>,
    asset: &VideoConversionJob,
    target_res: i32,
) -> Option<String> {
    let tone_map = should_tone_map_i16(config.tonemap, asset.color_transfer);
    let mut filters = Vec::new();
    if should_scale(asset, target_res) {
        filters.push(scale_filter(asset, target_res));
    }
    if tone_map {
        filters.push(tonemapx_filter(config.tonemap));
    } else if !asset.pixel_format.ends_with("420p") {
        filters.push("format=yuv420p".into());
    }
    if filters.is_empty() {
        None
    } else {
        Some(filters.join(","))
    }
}

fn should_scale(asset: &VideoConversionJob, target_res: i32) -> bool {
    let odd = asset.video_width % 2 != 0 || asset.video_height % 2 != 0;
    let larger = asset.video_width.min(asset.video_height) > target_res;
    odd || larger
}

fn scale_filter(asset: &VideoConversionJob, target_res: i32) -> String {
    if asset.video_height > asset.video_width {
        format!("{target_res}:-2")
    } else {
        format!("-2:{target_res}")
    }
}

fn scale_expression(asset: &VideoConversionJob, target_res: i32) -> String {
    if asset.video_height > asset.video_width {
        format!("{target_res}:-2")
    } else {
        format!("-2:{target_res}")
    }
}

fn append_rkmpp_level(args: &mut Vec<String>, codec: &str) {
    let level = if codec.eq_ignore_ascii_case("hevc") || codec.eq_ignore_ascii_case("h265") {
        "153"
    } else {
        "51"
    };
    args.extend(["-level".into(), level.into()]);
}

fn nvenc_preset(preset: &str) -> String {
    let presets = [
        "ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow",
    ];
    let index = presets
        .iter()
        .position(|value| value.eq_ignore_ascii_case(preset))
        .unwrap_or(0);
    let mapped = 7 - index.min(6);
    format!("p{mapped}")
}

fn qsv_preset(preset: &str) -> String {
    let presets = [
        "ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow",
    ];
    let index = presets
        .iter()
        .position(|value| value.eq_ignore_ascii_case(preset))
        .unwrap_or(3);
    (index.min(6) + 1).to_string()
}

fn vaapi_compression_level(preset: &str) -> String {
    qsv_preset(preset)
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
    } else {
        value * 1_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

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
            pixel_format: "yuv420p10le".into(),
            frame_count: 1000,
            frame_rate: Some(30.0),
            rotation: 0,
            color_transfer: Some(16),
            format_name: "mp4".into(),
            format_long_name: None,
            audio_index: Some(1),
            audio_codec_name: Some("aac".into()),
            files: Vec::new(),
        }
    }

    fn hw_config() -> VideoHwConfig<'static> {
        VideoHwConfig {
            accel: "nvenc",
            accel_decode: true,
            preferred_hw_device: "auto",
            tonemap: "hable",
            target_video_codec: "h264",
            preset: "fast",
            crf: 23,
            max_bitrate: "5000k",
        }
    }

    #[test]
    fn nvenc_encoder_resolves_for_h264() {
        assert_eq!(hw_video_encoder("nvenc", "h264"), Some("h264_nvenc"));
        assert_eq!(hw_video_encoder("rkmpp", "vp9"), None);
    }

    #[test]
    fn hw_filter_includes_tonemap_cuda_for_hdr_nvenc() {
        let config = hw_config();
        let asset = sample_asset();
        let filter = build_hw_video_filter(&config, &VideoInterfaces::default(), &asset, 720)
            .expect("filter");
        assert!(filter.contains("tonemap_cuda"));
        assert!(filter.contains("scale_cuda"));
    }

    #[test]
    fn append_hw_rate_uses_cq_for_nvenc() {
        let config = hw_config();
        let mut args = Vec::new();
        append_hw_rate_options(&mut args, &config);
        let joined = args.join(" ");
        assert!(joined.contains("-cq:v"));
        assert!(joined.contains("-preset"));
    }
}
