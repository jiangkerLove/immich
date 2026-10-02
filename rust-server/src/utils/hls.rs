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
    HlsVariant {
        resolution: 1440,
        codec: "av1",
        bitrate: 7_000_000,
        codec_string: "",
    },
    HlsVariant {
        resolution: 1440,
        codec: "hevc",
        bitrate: 8_000_000,
        codec_string: "",
    },
    HlsVariant {
        resolution: 1440,
        codec: "h264",
        bitrate: 14_000_000,
        codec_string: "",
    },
    HlsVariant {
        resolution: 2160,
        codec: "av1",
        bitrate: 12_000_000,
        codec_string: "",
    },
    HlsVariant {
        resolution: 2160,
        codec: "hevc",
        bitrate: 14_000_000,
        codec_string: "",
    },
    HlsVariant {
        resolution: 2160,
        codec: "h264",
        bitrate: 25_000_000,
        codec_string: "",
    },
];

struct CodecLevel {
    max_frame: f64,
    max_rate: f64,
    token: &'static str,
}

const H264_LEVELS: &[CodecLevel] = &[
    CodecLevel {
        max_frame: 1620.0,
        max_rate: 40_500.0,
        token: "1e",
    },
    CodecLevel {
        max_frame: 3600.0,
        max_rate: 108_000.0,
        token: "1f",
    },
    CodecLevel {
        max_frame: 5120.0,
        max_rate: 216_000.0,
        token: "20",
    },
    CodecLevel {
        max_frame: 8192.0,
        max_rate: 245_760.0,
        token: "28",
    },
    CodecLevel {
        max_frame: 8704.0,
        max_rate: 522_240.0,
        token: "2a",
    },
    CodecLevel {
        max_frame: 22_080.0,
        max_rate: 589_824.0,
        token: "32",
    },
    CodecLevel {
        max_frame: 36_864.0,
        max_rate: 983_040.0,
        token: "33",
    },
    CodecLevel {
        max_frame: 36_864.0,
        max_rate: 2_073_600.0,
        token: "34",
    },
    CodecLevel {
        max_frame: 139_264.0,
        max_rate: 4_177_920.0,
        token: "3c",
    },
    CodecLevel {
        max_frame: 139_264.0,
        max_rate: 8_355_840.0,
        token: "3d",
    },
    CodecLevel {
        max_frame: 139_264.0,
        max_rate: 16_711_680.0,
        token: "3e",
    },
];

const HEVC_LEVELS: &[CodecLevel] = &[
    CodecLevel {
        max_frame: 552_960.0,
        max_rate: 16_588_800.0,
        token: "L90",
    },
    CodecLevel {
        max_frame: 983_040.0,
        max_rate: 33_177_600.0,
        token: "L93",
    },
    CodecLevel {
        max_frame: 2_228_224.0,
        max_rate: 66_846_720.0,
        token: "L120",
    },
    CodecLevel {
        max_frame: 2_228_224.0,
        max_rate: 133_693_440.0,
        token: "L123",
    },
    CodecLevel {
        max_frame: 8_912_896.0,
        max_rate: 267_386_880.0,
        token: "L150",
    },
    CodecLevel {
        max_frame: 8_912_896.0,
        max_rate: 534_773_760.0,
        token: "L153",
    },
    CodecLevel {
        max_frame: 8_912_896.0,
        max_rate: 1_069_547_520.0,
        token: "L156",
    },
    CodecLevel {
        max_frame: 35_651_584.0,
        max_rate: 1_069_547_520.0,
        token: "L180",
    },
    CodecLevel {
        max_frame: 35_651_584.0,
        max_rate: 2_139_095_040.0,
        token: "L183",
    },
    CodecLevel {
        max_frame: 35_651_584.0,
        max_rate: 4_278_190_080.0,
        token: "L186",
    },
];

const AV1_LEVELS: &[CodecLevel] = &[
    CodecLevel {
        max_frame: 665_856.0,
        max_rate: 19_975_168.0,
        token: "04M",
    },
    CodecLevel {
        max_frame: 1_065_024.0,
        max_rate: 31_950_336.0,
        token: "05M",
    },
    CodecLevel {
        max_frame: 2_359_296.0,
        max_rate: 70_778_880.0,
        token: "08M",
    },
    CodecLevel {
        max_frame: 2_359_296.0,
        max_rate: 141_557_760.0,
        token: "09M",
    },
    CodecLevel {
        max_frame: 8_912_896.0,
        max_rate: 267_386_880.0,
        token: "12M",
    },
    CodecLevel {
        max_frame: 8_912_896.0,
        max_rate: 534_773_760.0,
        token: "13M",
    },
    CodecLevel {
        max_frame: 8_912_896.0,
        max_rate: 1_069_547_520.0,
        token: "14M",
    },
    CodecLevel {
        max_frame: 35_651_584.0,
        max_rate: 1_069_547_520.0,
        token: "16M",
    },
    CodecLevel {
        max_frame: 35_651_584.0,
        max_rate: 2_139_095_040.0,
        token: "17M",
    },
    CodecLevel {
        max_frame: 35_651_584.0,
        max_rate: 4_278_190_080.0,
        token: "18M",
    },
];

fn pick_level(levels: &[CodecLevel], frame: f64, rate: f64) -> &'static str {
    levels
        .iter()
        .find(|level| frame <= level.max_frame && rate <= level.max_rate)
        .or_else(|| levels.last())
        .map(|level| level.token)
        .unwrap_or("")
}

pub fn hls_codec_string(codec: &str, width: u32, height: u32, fps: f64) -> String {
    match codec {
        "h264" => {
            let macroblocks = f64::from(width.div_ceil(16) * height.div_ceil(16));
            format!(
                "avc1.6400{}",
                pick_level(H264_LEVELS, macroblocks, macroblocks * fps)
            )
        }
        "hevc" => {
            let samples = f64::from(width) * f64::from(height);
            format!(
                "hvc1.1.6.{}.B0",
                pick_level(HEVC_LEVELS, samples, samples * fps)
            )
        }
        "av1" => {
            let samples = f64::from(width) * f64::from(height);
            format!(
                "av01.0.{}.08",
                pick_level(AV1_LEVELS, samples, samples * fps)
            )
        }
        other => other.to_string(),
    }
}

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
