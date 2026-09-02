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
    pub dv_profile: Option<i16>,
    pub dv_level: Option<i16>,
    pub dv_bl_signal_compatibility_id: Option<i16>,
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

#[derive(Debug, Clone)]
pub struct ProbePackets {
    pub keyframe_pts: Vec<i32>,
    pub keyframe_acc_duration: Vec<i32>,
    pub keyframe_own_duration: Vec<i32>,
    pub total_duration: i32,
    pub packet_count: i32,
    pub output_frames: i32,
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
    dv_profile: Option<String>,
    dv_level: Option<String>,
    dv_bl_signal_compatibility_id: Option<String>,
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

    let format = parsed
        .format
        .ok_or_else(|| "missing ffprobe format".to_string())?;
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

pub async fn probe_packets(path: &str, stream_index: i32) -> Result<Option<ProbePackets>, String> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg(stream_index.to_string())
        .arg("-show_entries")
        .arg("packet=pts,duration,flags")
        .arg("-of")
        .arg("csv=p=0")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|err| format!("failed to run ffprobe packet scan: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe packet scan failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(parse_packets_csv(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_packets_csv(output: &str) -> Option<ProbePackets> {
    let mut total_duration = 0i64;
    let mut keyframe_pts = Vec::new();
    let mut keyframe_acc_duration = Vec::new();
    let mut keyframe_own_duration = Vec::new();
    let mut post_discard = Vec::new();

    for line in output.lines() {
        let mut fields = line.split(',');
        let (Some(pts), Some(duration), Some(flags)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let (Ok(pts), Ok(duration)) = (pts.parse::<i64>(), duration.parse::<i64>()) else {
            continue;
        };

        total_duration += duration;
        let flag_bytes = flags.as_bytes();
        if flag_bytes.get(1) != Some(&b'D') {
            post_discard.push((pts, duration));
        }
        if flag_bytes.first() == Some(&b'K') {
            keyframe_pts.push(pts);
            keyframe_acc_duration.push(total_duration);
            keyframe_own_duration.push(duration);
        }
    }

    if post_discard.is_empty() || total_duration <= 0 {
        return None;
    }

    let packet_count = post_discard.len();
    let output_frames = cfr_output_frames(
        &mut post_discard,
        packet_count as f64 / total_duration as f64,
    );
    Some(ProbePackets {
        keyframe_pts: keyframe_pts
            .into_iter()
            .map(|value| value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
            .collect(),
        keyframe_acc_duration: keyframe_acc_duration
            .into_iter()
            .map(|value| value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
            .collect(),
        keyframe_own_duration: keyframe_own_duration
            .into_iter()
            .map(|value| value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
            .collect(),
        total_duration: total_duration.clamp(0, i64::from(i32::MAX)) as i32,
        packet_count: packet_count.min(i32::MAX as usize) as i32,
        output_frames,
    })
}

fn cfr_output_frames(packets: &mut [(i64, i64)], slots_per_tick: f64) -> i32 {
    packets.sort_by_key(|(pts, _)| *pts);
    let first_pts = packets[0].0;
    let mut output_frames = 0i64;
    let mut next_pts = 0.0;
    let mut history = [0i64; 3];

    for (pts, duration) in packets {
        let sync_ipts = (*pts - first_pts) as f64 * slots_per_tick;
        let duration = *duration as f64 * slots_per_tick;
        let mut delta0 = sync_ipts - next_pts;
        let delta = delta0 + duration;
        if delta0 < 0.0 && delta > 0.0 {
            delta0 = 0.0;
        }
        let mut frames = 1i64;
        let mut previous = 0i64;
        if delta < -1.1 {
            frames = 0;
        } else if delta > 1.1 {
            frames = delta.round() as i64;
            if delta0 > 1.1 {
                previous = (delta0 - 0.6).round() as i64;
            }
        }
        output_frames += frames;
        next_pts += frames as f64;
        history = [previous, history[0], history[1]];
    }

    history.sort_unstable();
    (output_frames + history[1]).clamp(0, i64::from(i32::MAX)) as i32
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
        dv_profile: stream
            .dv_profile
            .as_deref()
            .and_then(|value| value.parse().ok()),
        dv_level: stream
            .dv_level
            .as_deref()
            .and_then(|value| value.parse().ok()),
        dv_bl_signal_compatibility_id: stream
            .dv_bl_signal_compatibility_id
            .as_deref()
            .and_then(|value| value.parse().ok()),
    }
}

fn parse_audio_stream(stream: &FfprobeStream) -> ProbeAudioStream {
    ProbeAudioStream {
        index: stream.index.unwrap_or(0),
        codec_name: stream
            .codec_name
            .clone()
            .unwrap_or_else(|| "unknown".into()),
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

#[cfg(test)]
mod tests {
    use super::parse_packets_csv;

    #[test]
    fn parses_keyframes_and_excludes_discarded_packets_from_count() {
        let packets = parse_packets_csv("0,100,K_\n100,100,__\n200,100,KD\n")
            .expect("packet data should parse");

        assert_eq!(packets.keyframe_pts, vec![0, 200]);
        assert_eq!(packets.keyframe_acc_duration, vec![100, 300]);
        assert_eq!(packets.keyframe_own_duration, vec![100, 100]);
        assert_eq!(packets.total_duration, 300);
        assert_eq!(packets.packet_count, 2);
        assert_eq!(packets.output_frames, 2);
    }

    #[test]
    fn rejects_packet_streams_without_usable_packets() {
        assert!(parse_packets_csv("N/A,N/A,__").is_none());
        assert!(parse_packets_csv("").is_none());
    }
}
