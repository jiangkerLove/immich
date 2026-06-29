pub const HLS_BACKPRESSURE_PAUSE_SEGMENTS: i32 = 30;
pub const HLS_BACKPRESSURE_RESUME_SEGMENTS: i32 = 15;
pub const HLS_CLEANUP_INTERVAL_MS: u64 = 60_000;
pub const HLS_INACTIVITY_TIMEOUT_MS: u64 = 5 * 60 * 1000;
pub const HLS_LEASE_DURATION_MS: u64 = 30 * 60 * 1000;
pub const HLS_SEGMENT_DURATION: f64 = 2.0;
pub const HLS_VERSION: u32 = 7;

pub const HLS_SEGMENT_FILENAME_REGEX: &str = r"^seg_(\d+)\.m4s$";

#[derive(Debug, Clone, Copy)]
pub struct HlsVariant {
    pub resolution: u32,
    pub codec: &'static str,
    pub bitrate: u32,
    pub codec_string: &'static str,
}

pub const HLS_VARIANTS: &[HlsVariant] = &[
    HlsVariant {
        resolution: 480,
        codec: "av1",
        bitrate: 1_000_000,
        codec_string: "av01.0.04M.08",
    },
    HlsVariant {
        resolution: 480,
        codec: "hevc",
        bitrate: 1_200_000,
        codec_string: "hvc1.1.6.L90.B0",
    },
    HlsVariant {
        resolution: 480,
        codec: "h264",
        bitrate: 2_500_000,
        codec_string: "avc1.64001e",
    },
    HlsVariant {
        resolution: 720,
        codec: "av1",
        bitrate: 2_000_000,
        codec_string: "av01.0.08M.08",
    },
    HlsVariant {
        resolution: 720,
        codec: "hevc",
        bitrate: 2_500_000,
        codec_string: "hvc1.1.6.L93.B0",
    },
    HlsVariant {
        resolution: 720,
        codec: "h264",
        bitrate: 5_000_000,
        codec_string: "avc1.64001f",
    },
    HlsVariant {
        resolution: 1080,
        codec: "av1",
        bitrate: 4_000_000,
        codec_string: "av01.0.09M.08",
    },
    HlsVariant {
        resolution: 1080,
        codec: "hevc",
        bitrate: 4_500_000,
        codec_string: "hvc1.1.6.L120.B0",
    },
    HlsVariant {
        resolution: 1080,
        codec: "h264",
        bitrate: 8_000_000,
        codec_string: "avc1.640028",
    },
];

pub fn supported_codecs_for_accel(accel: &str) -> &'static [&'static str] {
    match accel {
        "disabled" => &["h264", "hevc", "vp9", "av1"],
        "nvenc" => &["h264", "hevc", "av1"],
        "qsv" => &["h264", "hevc", "vp9", "av1"],
        "vaapi" => &["h264", "hevc", "vp9", "av1"],
        "rkmpp" => &["h264", "hevc"],
        "v4l2m2m" => &["h264", "hevc"],
        _ => &["h264"],
    }
}

pub fn hls_crf(codec: &str) -> u32 {
    match codec {
        "hevc" => 28,
        "vp9" => 31,
        "av1" => 35,
        _ => 23,
    }
}
