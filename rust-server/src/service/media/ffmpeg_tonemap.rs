pub const COLOR_TRANSFER_SMPTE2084: i32 = 16;
pub const COLOR_TRANSFER_ARIB_STD_B67: i32 = 18;
pub const COLOR_PRIMARIES_RESERVED: i16 = 0;
pub const COLOR_MATRIX_RESERVED: i16 = 3;
pub const COLOR_TRANSFER_RESERVED: i16 = 0;

pub fn is_hdr_color_transfer_i32(value: i32) -> bool {
    matches!(value, COLOR_TRANSFER_SMPTE2084 | COLOR_TRANSFER_ARIB_STD_B67)
}

pub fn is_hdr_color_transfer_name(value: &str) -> bool {
    matches!(value, "smpte2084" | "arib-std-b67")
}

pub fn should_tone_map(tonemap: &str, color_transfer: Option<&str>) -> bool {
    tonemap != "disabled"
        && color_transfer.is_some_and(is_hdr_color_transfer_name)
}

pub fn should_tone_map_i32(tonemap: &str, color_transfer: i32) -> bool {
    tonemap != "disabled" && is_hdr_color_transfer_i32(color_transfer)
}

pub fn should_tone_map_i16(tonemap: &str, color_transfer: Option<i16>) -> bool {
    tonemap != "disabled"
        && color_transfer.is_some_and(|value| is_hdr_color_transfer_i32(value as i32))
}

#[derive(Debug, Clone)]
pub struct VideoThumbnailStream {
    pub video_index: i32,
    pub codec_name: String,
    pub pixel_format: Option<String>,
    pub color_primaries: i16,
    pub color_transfer: i16,
    pub color_matrix: i16,
    pub format_name: Option<String>,
}

/// Input options for video thumbnail extraction (NestJS `ThumbnailConfig.getBaseInputOptions`).
pub fn append_video_thumbnail_input_args(args: &mut Vec<String>, stream: &VideoThumbnailStream) {
    let mpegts = stream
        .format_name
        .as_deref()
        .is_some_and(|name| name.eq_ignore_ascii_case("mpegts"));
    if mpegts {
        args.extend(["-sws_flags".into(), "accurate_rnd+full_chroma_int".into()]);
    } else {
        args.extend([
            "-skip_frame".into(),
            "nointra".into(),
            "-sws_flags".into(),
            "accurate_rnd+full_chroma_int".into(),
        ]);
    }

    let mut overrides = Vec::new();
    if stream.color_primaries == COLOR_PRIMARIES_RESERVED {
        overrides.push("colour_primaries=1");
    }
    if stream.color_matrix == COLOR_MATRIX_RESERVED {
        overrides.push("matrix_coefficients=1");
    }
    if stream.color_transfer == COLOR_TRANSFER_RESERVED {
        overrides.push("transfer_characteristics=1");
    }

    if !overrides.is_empty() {
        args.push(format!("-bsf:{}", stream.video_index));
        args.push(format!(
            "{}_metadata={}",
            stream.codec_name,
            overrides.join(":")
        ));
    }
}

pub fn tonemapx_filter(tonemap: &str) -> String {
    format!(
        "tonemapx=tonemap={tonemap}:desat=0:p=bt709:t=bt709:m=bt709:r=pc:peak=100:format=yuv420p"
    )
}

pub fn tonemap_cuda_filter(tonemap: &str) -> String {
    format!(
        "tonemap_cuda=desat=0:matrix=bt709:primaries=bt709:range=pc:tonemap={tonemap}:tonemap_mode=lum:transfer=bt709:peak=100:format=nv12"
    )
}

pub fn tonemap_opencl_vaapi_chain(tonemap: &str) -> String {
    format!(
        "hwmap=derive_device=opencl,\
         tonemap_opencl=desat=0:format=nv12:matrix=bt709:primaries=bt709:transfer=bt709:range=pc:tonemap={tonemap}:tonemap_mode=lum:peak=100,\
         hwmap=derive_device=vaapi:reverse=1,format=vaapi"
    )
}

pub fn tonemap_opencl_qsv_chain(tonemap: &str) -> String {
    format!(
        "hwmap=derive_device=opencl,\
         tonemap_opencl=desat=0:format=nv12:matrix=bt709:primaries=bt709:transfer=bt709:range=pc:tonemap={tonemap}:tonemap_mode=lum:peak=100,\
         hwmap=derive_device=qsv:reverse=1,format=qsv"
    )
}

/// Video thumbnail filter chain aligned with NestJS `ThumbnailConfig.getFilterOptions`.
pub fn build_video_thumbnail_vf(
    tonemap: &str,
    stream: &VideoThumbnailStream,
    target_size: u32,
) -> String {
    let scale = format!(
        "scale={target_size}:{target_size}:force_original_aspect_ratio=decrease:flags=lanczos+accurate_rnd+full_chroma_int:out_range=pc"
    );
    let mut filters = vec![
        "fps=12:start_time=0:eof_action=pass:round=down".into(),
        "thumbnail=12".into(),
        r"select=gt(scene\,0.1)-eq(prev_selected_n\,n)+isnan(prev_selected_n)+gt(n\,20)".into(),
        "trim=end_frame=2".into(),
        "reverse".into(),
        scale,
    ];

    if should_tone_map_i16(tonemap, Some(stream.color_transfer)) {
        filters.push(tonemapx_filter(tonemap));
    } else if stream
        .pixel_format
        .as_deref()
        .is_some_and(|format| !format.ends_with("420p"))
    {
        filters.push("format=yuv420p".into());
    }

    filters.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_hdr_transfer_names() {
        assert!(should_tone_map("hable", Some("smpte2084")));
        assert!(!should_tone_map("disabled", Some("smpte2084")));
        assert!(!should_tone_map("hable", Some("bt709")));
    }

    #[test]
    fn thumbnail_vf_includes_tonemap_for_hdr() {
        let stream = sample_stream();
        let mut hdr = stream.clone();
        hdr.color_transfer = COLOR_TRANSFER_SMPTE2084 as i16;
        let vf = build_video_thumbnail_vf("hable", &hdr, 250);
        assert!(vf.contains("thumbnail=12"));
        assert!(vf.contains("tonemapx=tonemap=hable"));
    }

    #[test]
    fn bsf_metadata_for_reserved_color_values() {
        let stream = VideoThumbnailStream {
            video_index: 0,
            codec_name: "h264".into(),
            pixel_format: Some("yuv420p".into()),
            color_primaries: COLOR_PRIMARIES_RESERVED,
            color_transfer: COLOR_TRANSFER_RESERVED,
            color_matrix: COLOR_MATRIX_RESERVED,
            format_name: Some("mov".into()),
        };
        let mut args = Vec::new();
        append_video_thumbnail_input_args(&mut args, &stream);
        let joined = args.join(" ");
        assert!(joined.contains("-skip_frame"));
        assert!(joined.contains("-bsf:0"));
        assert!(joined.contains("h264_metadata=colour_primaries=1:matrix_coefficients=1:transfer_characteristics=1"));
    }

    fn sample_stream() -> VideoThumbnailStream {
        VideoThumbnailStream {
            video_index: 0,
            codec_name: "hevc".into(),
            pixel_format: Some("yuv420p10le".into()),
            color_primaries: 1,
            color_transfer: 1,
            color_matrix: 1,
            format_name: Some("mp4".into()),
        }
    }
}
