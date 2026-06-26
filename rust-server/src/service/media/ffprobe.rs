use std::process::Stdio;

use serde::Deserialize;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ProbeFormat {
    pub format_name: String,
    pub format_long_name: String,
    pub duration: Option<f64>,
    pub bitrate: i64,
}

#[derive(Debug, Clone)]
pub struct ProbeVideoStream {
    pub index: i32,
    pub codec_name: String,
    pub profile: Option<i32>,
    pub level: Option<i32>,
    pub width: i32,
    pub height: i32,
    pub bitrate: i64,
    pub frame_count: i64,
    pub frame_rate: Option<f64>,
    pub time_base_den: i32,
    pub rotation: i32,
    pub pixel_format: String,
    pub color_primaries: i16,
    pub color_transfer: i16,
    pub color_matrix: i16,
}

#[derive(Debug, Clone)]
pub struct ProbeAudioStream {
    pub index: i32,
    pub codec_name: String,
    pub profile: Option<i32>,
    pub bitrate: i64,
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub format: ProbeFormat,
    pub video: Option<ProbeVideoStream>,
    pub audio: Option<ProbeAudioStream>,
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    streams: Option<Vec<FfprobeStream>>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    index: Option<i32>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    profile: Option<String>,
    level: Option<i32>,
    width: Option<i32>,
    height: Option<i32>,
    bit_rate: Option<String>,
    nb_read_packets: Option<String>,
    nb_frames: Option<String>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    time_base: Option<String>,
    rotation: Option<String>,
    pix_fmt: Option<String>,
    color_primaries: Option<String>,
    color_transfer: Option<String>,
    color_space: Option<String>,
    disposition: Option<FfprobeDisposition>,
}

#[derive(Debug, Deserialize)]
struct FfprobeDisposition {
    attached_pic: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    format_name: Option<String>,
    format_long_name: Option<String>,
    duration: Option<String>,
    bit_rate: Option<String>,
}

pub async fn probe(path: &str) -> Result<ProbeResult, String> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg("-count_packets")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|err| format!("failed to run ffprobe: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe failed: {stderr}"));
    }

    let parsed: FfprobeOutput =
        serde_json::from_slice(&output.stdout).map_err(|err| err.to_string())?;

    let format = parsed.format.ok_or_else(|| "missing ffprobe format".to_string())?;
    let streams = parsed.streams.unwrap_or_default();

    let video = streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .filter(|stream| stream.disposition.as_ref().and_then(|d| d.attached_pic) != Some(1))
        .min_by_key(|stream| stream.index.unwrap_or(i32::MAX))
        .map(parse_video_stream);

    let audio = streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .min_by_key(|stream| stream.index.unwrap_or(i32::MAX))
        .map(parse_audio_stream);

    Ok(ProbeResult {
        format: ProbeFormat {
            format_name: format.format_name.unwrap_or_else(|| "unknown".into()),
            format_long_name: format.format_long_name.unwrap_or_default(),
            duration: format.duration.and_then(|v| v.parse().ok()),
            bitrate: parse_i64(format.bit_rate.as_deref()),
        },
        video,
        audio,
    })
}

fn parse_video_stream(stream: &FfprobeStream) -> ProbeVideoStream {
    let codec_name = stream
        .codec_name
        .as_deref()
        .unwrap_or("unknown")
        .replace("h265", "hevc");
    ProbeVideoStream {
        index: stream.index.unwrap_or(0),
        codec_name,
        profile: stream.profile.as_deref().and_then(parse_profile),
        level: stream.level,
        width: stream.width.unwrap_or(0),
        height: stream.height.unwrap_or(0),
        bitrate: parse_i64(stream.bit_rate.as_deref()),
        frame_count: stream
            .nb_read_packets
            .as_deref()
            .or(stream.nb_frames.as_deref())
            .map(|value| parse_i64(Some(value)))
            .unwrap_or(0),
        frame_rate: stream
            .avg_frame_rate
            .as_deref()
            .or(stream.r_frame_rate.as_deref())
            .and_then(parse_frame_rate),
        time_base_den: stream
            .time_base
            .as_deref()
            .and_then(parse_rational_den)
            .unwrap_or(1),
        rotation: stream
            .rotation
            .as_deref()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        pixel_format: stream.pix_fmt.clone().unwrap_or_else(|| "yuv420p".into()),
        color_primaries: map_color_primaries(stream.color_primaries.as_deref()),
        color_transfer: map_color_transfer(stream.color_transfer.as_deref()),
        color_matrix: map_color_matrix(stream.color_space.as_deref()),
    }
}

fn parse_audio_stream(stream: &FfprobeStream) -> ProbeAudioStream {
    ProbeAudioStream {
        index: stream.index.unwrap_or(0),
        codec_name: stream.codec_name.clone().unwrap_or_else(|| "unknown".into()),
        profile: stream.profile.as_deref().and_then(parse_profile),
        bitrate: parse_i64(stream.bit_rate.as_deref()),
    }
}

fn parse_i64(value: Option<&str>) -> i64 {
    value.and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn parse_frame_rate(value: &str) -> Option<f64> {
    if value.contains('/') {
        let mut parts = value.split('/');
        let num: f64 = parts.next()?.parse().ok()?;
        let den: f64 = parts.next()?.parse().ok()?;
        if den == 0.0 {
            return None;
        }
        return Some(num / den);
    }
    value.parse().ok()
}

fn parse_rational_den(value: &str) -> Option<i32> {
    if value.contains('/') {
        return value.split('/').nth(1)?.parse().ok();
    }
    None
}

fn parse_profile(value: &str) -> Option<i32> {
    if let Ok(v) = value.parse::<i32>() {
        return Some(v);
    }
    match value.to_ascii_lowercase().as_str() {
        "baseline" => Some(66),
        "main" => Some(77),
        "high" => Some(100),
        "lc" => Some(0),
        _ => None,
    }
}

fn map_color_primaries(value: Option<&str>) -> i16 {
    match value.unwrap_or("") {
        "bt709" => 1,
        "bt470m" => 4,
        "bt470bg" => 5,
        "smpte170m" => 6,
        "smpte240m" => 7,
        "film" => 8,
        "bt2020" => 9,
        _ => 2,
    }
}

fn map_color_transfer(value: Option<&str>) -> i16 {
    match value.unwrap_or("") {
        "bt709" => 1,
        "smpte2084" => 16,
        "arib-std-b67" => 18,
        "bt2020-10" => 14,
        "bt2020-12" => 15,
        _ => 2,
    }
}

fn map_color_matrix(value: Option<&str>) -> i16 {
    match value.unwrap_or("") {
        "bt709" => 1,
        "bt470bg" => 5,
        "smpte170m" => 6,
        "bt2020nc" => 9,
        _ => 2,
    }
}
