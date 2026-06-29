use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Weak};
use std::time::Duration;

use chrono::{DateTime, Utc};
use notify::{EventKind, RecursiveMode, Watcher};
use regex::Regex;
use serde::Serialize;
use sqlx::PgPool;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::task::AbortHandle;
use uuid::Uuid;

use crate::models::db::system_metadata::get_json;
use crate::models::db::video_stream::{self, VideoStreamAssetRow};
use crate::models::response::response::ErrorResp;
use crate::service::media::hls_encode::{build_hls_ffmpeg_args, HlsFfmpegSettings};
use crate::utils::hls::{
    supported_codecs_for_accel, HLS_BACKPRESSURE_PAUSE_SEGMENTS, HLS_BACKPRESSURE_RESUME_SEGMENTS,
    HLS_CLEANUP_INTERVAL_MS, HLS_INACTIVITY_TIMEOUT_MS, HLS_LEASE_DURATION_MS,
    HLS_SEGMENT_DURATION, HLS_VARIANTS, HLS_VERSION,
};
use crate::utils::pending_events::PendingEvents;
use crate::utils::storage::StoragePaths;
use crate::utils::system_config::json_str;
use crate::utils::video_interfaces::detect_video_interfaces;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HlsSessionResult {
    pub session_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HlsSegmentResult {
    pub session_id: Uuid,
    pub variant_index: u32,
    pub segment_index: i32,
}

#[derive(Debug, Default, Clone)]
pub struct ApiSession {
    pub last_requested_segment: Option<i32>,
    pub last_variant_index: Option<u32>,
}

struct TranscodeProcess {
    pid: u32,
    variant_index: u32,
    watcher_abort: AbortHandle,
}

struct TranscodeSession {
    asset_id: Uuid,
    owner_id: Uuid,
    expires_at: DateTime<Utc>,
    last_activity: DateTime<Utc>,
    variant_index: Option<u32>,
    start_segment: Option<i32>,
    last_completed_segment: Option<i32>,
    last_client_requested_segment: Option<i32>,
    paused: bool,
    starting: bool,
    process: Option<TranscodeProcess>,
}

struct SegmentReady {
    session_id: Uuid,
    variant_index: u32,
    segment_index: i32,
}

pub struct HlsEngine {
    pool: PgPool,
    storage: StoragePaths,
    pending_sessions: PendingEvents<HlsSessionResult>,
    pending_segments: PendingEvents<HlsSegmentResult>,
    api_sessions: Mutex<HashMap<Uuid, ApiSession>>,
    transcode_sessions: Mutex<HashMap<Uuid, TranscodeSession>>,
    segment_regex: Regex,
    segment_ready_tx: mpsc::UnboundedSender<SegmentReady>,
    self_arc: Weak<HlsEngine>,
}

impl HlsEngine {
    pub fn spawn(pool: PgPool, storage: StoragePaths) -> Arc<Self> {
        let (segment_ready_tx, mut segment_ready_rx) = mpsc::unbounded_channel();

        let engine = Arc::new_cyclic(|weak| Self {
            pool,
            storage,
            pending_sessions: PendingEvents::new(5_000),
            pending_segments: PendingEvents::new(15_000),
            api_sessions: Mutex::new(HashMap::new()),
            transcode_sessions: Mutex::new(HashMap::new()),
            segment_regex: Regex::new(crate::utils::hls::HLS_SEGMENT_FILENAME_REGEX)
                .expect("valid segment regex"),
            segment_ready_tx,
            self_arc: weak.clone(),
        });

        let completion_engine = engine.clone();
        tokio::spawn(async move {
            while let Some(event) = segment_ready_rx.recv().await {
                completion_engine
                    .on_segment_ready(event.session_id, event.variant_index, event.segment_index)
                    .await;
            }
        });

        let cleanup = engine.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(HLS_CLEANUP_INTERVAL_MS));
            loop {
                ticker.tick().await;
                cleanup.remove_inactive_sessions().await;
            }
        });

        engine
    }

    pub async fn request_session(
        &self,
        asset_id: Uuid,
        owner_id: Uuid,
    ) -> Result<Uuid, ErrorResp> {
        let session_id = Uuid::new_v4();
        let wait = self.pending_sessions.wait(session_id.to_string());
        self.handle_session_request(session_id, asset_id, owner_id)
            .await;
        let result = wait
            .await
            .map_err(ErrorResp::ServerError)?;
        if let Some(error) = result.error {
            return Err(ErrorResp::ServerError(error));
        }
        self.track_api_session(session_id, None).await;
        Ok(session_id)
    }

    pub async fn end_session(&self, session_id: Uuid) {
        self.handle_session_end(session_id).await;
    }

    pub async fn heartbeat(&self, session_id: Uuid, segment_index: Option<i32>) {
        let mut sessions = self.transcode_sessions.lock().await;
        let Some(session) = sessions.get_mut(&session_id) else {
            return;
        };

        session.last_activity = Utc::now();

        if let Some(segment_index) = segment_index {
            session.last_client_requested_segment = Some(segment_index);
            apply_backpressure(session);
        }

        let remaining = session.expires_at - Utc::now();
        if remaining.num_milliseconds() < (HLS_LEASE_DURATION_MS / 2) as i64 {
            session.expires_at =
                Utc::now() + chrono::Duration::milliseconds(HLS_LEASE_DURATION_MS as i64);
            let expires_at = session.expires_at;
            drop(sessions);
            let _ = video_stream::extend_session(&self.pool, &session_id, expires_at).await;
        }
    }

    pub async fn wait_for_segment(
        &self,
        session_id: Uuid,
        variant_index: u32,
        segment_index: i32,
        _asset_id: Uuid,
    ) -> Result<(), ErrorResp> {
        let owner_id = self
            .transcode_sessions
            .lock()
            .await
            .get(&session_id)
            .map(|session| session.owner_id);
        let Some(owner_id) = owner_id else {
            return Err(ErrorResp::NotFound("HLS session not found".to_string()));
        };

        if segment_file_exists(
            &self.storage,
            owner_id,
            session_id,
            variant_index,
            segment_index,
        )
        .await
        {
            return Ok(());
        }

        self.ensure_transcode(session_id, variant_index, segment_index)
            .await;

        self.pending_segments
            .wait(segment_key(session_id, variant_index, segment_index))
            .await
            .map(|_| ())
            .map_err(ErrorResp::ServerError)
    }

    pub async fn track_api_session(&self, session_id: Uuid, variant_index: Option<u32>) -> ApiSession {
        let mut sessions = self.api_sessions.lock().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            if session.last_variant_index.is_some()
                && session.last_variant_index != variant_index
            {
                self.pending_segments
                    .reject_by_prefix(
                        &format!("{}:{}:", session_id, session.last_variant_index.unwrap_or(0)),
                        "Variant changed",
                    )
                    .await;
            }
            session.last_variant_index = variant_index;
            return session.clone();
        }

        let session = ApiSession {
            last_requested_segment: None,
            last_variant_index: variant_index,
        };
        sessions.insert(session_id, session.clone());
        session
    }

    pub fn segment_index_from_filename(
        &self,
        session: &mut ApiSession,
        filename: &str,
        init_segment: Option<i32>,
    ) -> i32 {
        if filename.ends_with(".mp4") {
            return init_segment.unwrap_or(session.last_requested_segment.unwrap_or(-1) + 1);
        }
        if let Some(caps) = self.segment_regex.captures(filename) {
            let index = caps[1].parse::<i32>().unwrap_or(0);
            session.last_requested_segment = Some(index);
            return index;
        }
        0
    }

    pub async fn prewarm_variant(
        &self,
        _asset_id: Uuid,
        session_id: Uuid,
        variant_index: u32,
        hinted_segment: Option<i32>,
    ) {
        let (owner_id, session) = {
            let api_sessions = self.api_sessions.lock().await;
            let transcode_sessions = self.transcode_sessions.lock().await;
            let Some(api_session) = api_sessions.get(&session_id).cloned() else {
                return;
            };
            let Some(transcode_session) = transcode_sessions.get(&session_id) else {
                return;
            };
            (transcode_session.owner_id, api_session)
        };

        if session.last_variant_index == Some(variant_index) {
            return;
        }

        let next_segment = session.last_requested_segment.map(|value| value + 1);
        if let Some(segment_index) = hinted_segment.or(next_segment) {
            if segment_file_exists(
                &self.storage,
                owner_id,
                session_id,
                variant_index,
                segment_index,
            )
            .await
            {
                return;
            }
            self.ensure_transcode(session_id, variant_index, segment_index)
                .await;
        }
    }

    pub async fn generate_main_playlist(
        &self,
        session_id: Uuid,
        asset: &VideoStreamAssetRow,
    ) -> Result<String, ErrorResp> {
        let config = get_json(&self.pool, "system-config")
            .await?
            .unwrap_or_default();
        let accel = json_str(&config, &["ffmpeg", "accel"], "disabled");
        let supported = supported_codecs_for_accel(&accel);

        let (fps, _, _, _) = video_stream::segmentation(asset);
        let source_resolution = asset.width.min(asset.height).max(0) as u32;
        let target_resolution = source_resolution.max(HLS_VARIANTS[0].resolution);

        let mut lines = vec![
            "#EXTM3U".to_string(),
            format!("#EXT-X-VERSION:{HLS_VERSION}"),
            "#EXT-X-INDEPENDENT-SEGMENTS".to_string(),
        ];

        for (index, variant) in HLS_VARIANTS.iter().enumerate() {
            if variant.resolution > target_resolution
                || !supported.iter().any(|codec| *codec == variant.codec)
            {
                continue;
            }
            let (width, height) = video_stream::output_size(
                asset.width,
                asset.height,
                asset.orientation,
                variant.resolution,
            );
            lines.push(format!(
                "#EXT-X-STREAM-INF:BANDWIDTH={},RESOLUTION={}x{},CODECS=\"{}\",mp4a.40.2\",VIDEO-RANGE=SDR,FRAME-RATE={fps:.3}",
                variant.bitrate, width, height, variant.codec_string
            ));
            lines.push(format!("{session_id}/{index}/playlist.m3u8"));
        }
        lines.push(String::new());

        if lines.len() <= 4 {
            return Err(ErrorResp::NotFound(
                "No supported variants for this video".to_string(),
            ));
        }

        Ok(lines.join("\n"))
    }

    pub fn generate_media_playlist(asset: &VideoStreamAssetRow) -> String {
        let (fps, frames_per_segment, segment_count, full_segment_duration) =
            video_stream::segmentation(asset);
        let last_segment_frames = asset.output_frames - frames_per_segment * (segment_count - 1);
        let last_segment_duration = last_segment_frames as f64 / fps.max(0.001);

        let mut lines = vec![
            "#EXTM3U".to_string(),
            format!("#EXT-X-VERSION:{HLS_VERSION}"),
            "#EXT-X-INDEPENDENT-SEGMENTS".to_string(),
            format!("#EXT-X-TARGETDURATION:{}", HLS_SEGMENT_DURATION as u32),
            "#EXT-X-MEDIA-SEQUENCE:0".to_string(),
            "#EXT-X-PLAYLIST-TYPE:VOD".to_string(),
            "#EXT-X-MAP:URI=\"init.mp4\"".to_string(),
        ];

        for index in 0..segment_count - 1 {
            lines.push(format!("#EXTINF:{full_segment_duration:.6},"));
            lines.push(format!("seg_{index}.m4s"));
        }
        lines.push(format!("#EXTINF:{last_segment_duration:.6},"));
        lines.push(format!("seg_{}.m4s", segment_count - 1));
        lines.push("#EXT-X-ENDLIST".to_string());
        lines.push(String::new());
        lines.join("\n")
    }

    pub fn position_to_segment(asset: &VideoStreamAssetRow, position: f64) -> i32 {
        let (_, _, segment_count, segment_duration) = video_stream::segmentation(asset);
        position
            .div_euclid(segment_duration)
            .floor()
            .clamp(0.0, (segment_count - 1) as f64) as i32
    }

    pub async fn shutdown(&self) {
        let session_ids: Vec<Uuid> = self
            .transcode_sessions
            .lock()
            .await
            .keys()
            .copied()
            .collect();
        for session_id in session_ids {
            self.handle_session_end(session_id).await;
        }
    }

    async fn handle_session_request(&self, session_id: Uuid, asset_id: Uuid, owner_id: Uuid) {
        let expires_at = Utc::now() + chrono::Duration::milliseconds(HLS_LEASE_DURATION_MS as i64);
        match video_stream::create_session(&self.pool, &session_id, &asset_id, expires_at).await {
            Ok(()) => {
                self.transcode_sessions.lock().await.insert(
                    session_id,
                    TranscodeSession {
                        asset_id,
                        owner_id,
                        expires_at,
                        last_activity: Utc::now(),
                        variant_index: None,
                        start_segment: None,
                        last_completed_segment: None,
                        last_client_requested_segment: None,
                        paused: false,
                        starting: false,
                        process: None,
                    },
                );
                self.pending_sessions
                    .complete(
                        &session_id.to_string(),
                        HlsSessionResult {
                            session_id,
                            error: None,
                        },
                    )
                    .await;
            }
            Err(err) if is_unique_violation(&err) => {}
            Err(err) => {
                eprintln!("Failed to create HLS session {session_id}: {err}");
                self.pending_sessions
                    .complete(
                        &session_id.to_string(),
                        HlsSessionResult {
                            session_id,
                            error: Some("Failed to create HLS session".to_string()),
                        },
                    )
                    .await;
            }
        }
    }

    async fn handle_session_end(&self, session_id: Uuid) {
        self.api_sessions.lock().await.remove(&session_id);
        self.pending_segments
            .reject_by_prefix(&format!("{session_id}:"), "Session ended")
            .await;

        self.stop_transcode(session_id).await;

        let owner_id = {
            let mut sessions = self.transcode_sessions.lock().await;
            sessions.remove(&session_id).map(|session| session.owner_id)
        };

        if let Some(owner_id) = owner_id {
            let dir = self.storage.hls_session_folder(&owner_id, &session_id);
            let _ = tokio::fs::remove_dir_all(&dir).await;
        }
        let _ = video_stream::delete_session(&self.pool, &session_id).await;
    }

    async fn ensure_transcode(
        &self,
        session_id: Uuid,
        variant_index: u32,
        segment_index: i32,
    ) {
        let (needs_stop, should_start) = {
            let mut sessions = self.transcode_sessions.lock().await;
            let Some(session) = sessions.get_mut(&session_id) else {
                return;
            };

            session.variant_index.get_or_insert(variant_index);
            session.start_segment.get_or_insert(segment_index);

            let cur_segment = session
                .last_completed_segment
                .map(|value| value + 1)
                .unwrap_or(session.start_segment.unwrap_or(segment_index));
            let needs_restart = session.variant_index != Some(variant_index)
                || segment_index < session.start_segment.unwrap_or(segment_index)
                || segment_index > cur_segment + 1;

            if needs_restart {
                session.variant_index = Some(variant_index);
                session.start_segment = Some(segment_index);
                session.last_completed_segment = None;
                session.starting = true;
                (true, true)
            } else if session.process.is_some() {
                if session.paused {
                    resume_transcode(session);
                }
                (false, false)
            } else if session.starting {
                (false, false)
            } else {
                session.starting = true;
                (false, true)
            }
        };

        if needs_stop {
            self.stop_transcode(session_id).await;
        }

        if should_start {
            let result = self
                .start_transcode(session_id, variant_index, segment_index)
                .await;
            if let Err(err) = result {
                eprintln!("HLS transcode failed for session {session_id}: {err}");
                self.fail_session(session_id, err).await;
            }
            let mut sessions = self.transcode_sessions.lock().await;
            if let Some(session) = sessions.get_mut(&session_id) {
                session.starting = false;
            }
        }
    }

    async fn start_transcode(
        &self,
        session_id: Uuid,
        variant_index: u32,
        start_segment: i32,
    ) -> Result<(), String> {
        let (asset_id, owner_id) = {
            let sessions = self.transcode_sessions.lock().await;
            let Some(session) = sessions.get(&session_id) else {
                return Ok(());
            };
            if session.variant_index != Some(variant_index)
                || session.start_segment != Some(start_segment)
            {
                return Ok(());
            }
            (session.asset_id, session.owner_id)
        };

        let Some(asset) = video_stream::get_for_transcoding(&self.pool, &asset_id)
            .await
            .map_err(|err| err.to_string())?
        else {
            return Err(format!("Asset {asset_id} not found for HLS transcoding"));
        };

        let Some(variant) = HLS_VARIANTS.get(variant_index as usize) else {
            return Err(format!("Invalid variant index {variant_index}"));
        };

        let variant_dir = self
            .storage
            .hls_variant_folder(&owner_id, &session_id, variant_index);
        let _ = tokio::fs::remove_dir_all(&variant_dir).await;
        tokio::fs::create_dir_all(&variant_dir)
            .await
            .map_err(|err| err.to_string())?;

        let fps =
            (asset.packet_count as f64 * asset.time_base as f64) / asset.total_duration.max(1) as f64;
        let gop = (HLS_SEGMENT_DURATION * fps).ceil() as i32;
        let seek_seconds = if start_segment > 0 {
            (start_segment as f64 * gop as f64 - 0.5) / fps.max(0.001)
        } else {
            0.0
        };

        let config = get_json(&self.pool, "system-config")
            .await
            .map_err(|err| err.to_string())?
            .unwrap_or_default();
        let settings = HlsFfmpegSettings::from_config(&config);
        let interfaces = detect_video_interfaces();

        let args = build_hls_ffmpeg_args(
            &settings,
            &interfaces,
            &asset,
            variant,
            &asset.original_path,
            &variant_dir,
            start_segment,
            gop,
            seek_seconds,
        )
        .map_err(|err| err.to_string())?;

        println!(
            "Starting HLS transcode for asset {asset_id} variant {variant_index}: ffmpeg {}",
            args[1..].join(" ")
        );

        let segment_regex = self.segment_regex.clone();
        let segment_ready_tx = self.segment_ready_tx.clone();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        let watcher = notify::recommended_watcher(move |result| {
            if let Ok(event) = result {
                let _ = event_tx.send(event);
            }
        })
        .map_err(|err| err.to_string())?;
        let mut watcher = watcher;
        watcher
            .watch(&variant_dir, RecursiveMode::NonRecursive)
            .map_err(|err| err.to_string())?;

        let mut child = Command::new(&args[0])
            .args(&args[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| err.to_string())?;

        let pid = child.id().ok_or_else(|| "ffmpeg process has no pid".to_string())?;

        let watcher_dir = variant_dir.clone();
        let watcher_handle = tokio::spawn(async move {
            let _watcher = watcher;
            while let Some(event) = event_rx.recv().await {
                if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    continue;
                }
                for path in event.paths {
                    if !path.starts_with(&watcher_dir) {
                        continue;
                    }
                    let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
                        continue;
                    };
                    let Some(caps) = segment_regex.captures(filename) else {
                        continue;
                    };
                    let index = caps[1].parse::<i32>().unwrap_or(-1);
                    let _ = segment_ready_tx.send(SegmentReady {
                        session_id,
                        variant_index,
                        segment_index: index,
                    });
                }
            }
        });

        let engine = self
            .self_arc
            .upgrade()
            .expect("HlsEngine dropped while transcode running");
        let stderr = child.stderr.take();
        tokio::spawn(async move {
            if let Some(mut stderr) = stderr {
                use tokio::io::AsyncReadExt;
                let mut buf = Vec::new();
                let _ = stderr.read_to_end(&mut buf).await;
                if !buf.is_empty() {
                    eprintln!(
                        "HLS ffmpeg stderr for session {session_id} variant {variant_index}: {}",
                        String::from_utf8_lossy(&buf)
                    );
                }
            }
        });

        tokio::spawn(async move {
            let status = child.wait().await;
            engine
                .on_process_exit(session_id, variant_index, pid, status)
                .await;
        });

        let mut sessions = self.transcode_sessions.lock().await;
        let Some(session) = sessions.get_mut(&session_id) else {
            let _ = signal_process(pid, libc::SIGKILL);
            watcher_handle.abort();
            return Ok(());
        };
        session.process = Some(TranscodeProcess {
            pid,
            variant_index,
            watcher_abort: watcher_handle.abort_handle(),
        });

        Ok(())
    }

    async fn on_segment_ready(
        &self,
        session_id: Uuid,
        variant_index: u32,
        segment_index: i32,
    ) {
        let expected = {
            let mut sessions = self.transcode_sessions.lock().await;
            let Some(session) = sessions.get_mut(&session_id) else {
                return;
            };
            let Some(process) = session.process.as_ref() else {
                return;
            };
            if process.variant_index != variant_index {
                return;
            }
            let expected = session
                .last_completed_segment
                .map(|value| value + 1)
                .unwrap_or(session.start_segment.unwrap_or(segment_index));
            if segment_index != expected {
                return;
            }
            session.last_completed_segment = Some(segment_index);
            apply_backpressure(session);
            expected
        };

        let _ = expected;
        self.pending_segments
            .complete(
                &segment_key(session_id, variant_index, segment_index),
                HlsSegmentResult {
                    session_id,
                    variant_index,
                    segment_index,
                },
            )
            .await;
    }

    async fn on_process_exit(
        &self,
        session_id: Uuid,
        variant_index: u32,
        pid: u32,
        status: Result<std::process::ExitStatus, std::io::Error>,
    ) {
        let should_fail = {
            let mut sessions = self.transcode_sessions.lock().await;
            let Some(session) = sessions.get_mut(&session_id) else {
                return;
            };
            let Some(process) = session.process.as_ref() else {
                return;
            };
            if process.pid != pid || process.variant_index != variant_index {
                return;
            }
            process.watcher_abort.abort();
            session.paused = false;
            session.process = None;
            session.last_completed_segment = None;

            match status {
                Ok(exit) if exit.success() => false,
                Ok(exit) => {
                    eprintln!(
                        "FFmpeg exited with code {:?} for session {session_id} variant {variant_index}",
                        exit.code()
                    );
                    true
                }
                Err(err) => {
                    eprintln!(
                        "FFmpeg wait failed for session {session_id} variant {variant_index}: {err}"
                    );
                    true
                }
            }
        };

        if should_fail {
            self.fail_session(
                session_id,
                "Transcoding process exited unexpectedly".to_string(),
            )
            .await;
        }
    }

    async fn stop_transcode(&self, session_id: Uuid) {
        let process = {
            let mut sessions = self.transcode_sessions.lock().await;
            let Some(session) = sessions.get_mut(&session_id) else {
                return;
            };
            session.last_completed_segment = None;
            session.paused = false;
            session.process.take()
        };

        if let Some(process) = process {
            process.watcher_abort.abort();
            let _ = signal_process(process.pid, libc::SIGKILL);
        }
    }

    async fn fail_session(&self, session_id: Uuid, error: String) {
        self.pending_sessions
            .complete(
                &session_id.to_string(),
                HlsSessionResult {
                    session_id,
                    error: Some(error),
                },
            )
            .await;
        self.handle_session_end(session_id).await;
    }

    async fn remove_inactive_sessions(&self) {
        let cutoff = Utc::now() - chrono::Duration::milliseconds(HLS_INACTIVITY_TIMEOUT_MS as i64);
        let inactive: Vec<Uuid> = self
            .transcode_sessions
            .lock()
            .await
            .iter()
            .filter(|(_, session)| session.last_activity < cutoff)
            .map(|(id, _)| *id)
            .collect();

        for session_id in inactive {
            self.handle_session_end(session_id).await;
        }
    }
}

fn apply_backpressure(session: &mut TranscodeSession) {
    let (Some(completed), Some(requested)) = (
        session.last_completed_segment,
        session.last_client_requested_segment,
    ) else {
        return;
    };

    let lead = completed - requested;
    if !session.paused && lead > HLS_BACKPRESSURE_PAUSE_SEGMENTS {
        pause_transcode(session);
    } else if session.paused && lead < HLS_BACKPRESSURE_RESUME_SEGMENTS {
        resume_transcode(session);
    }
}

fn pause_transcode(session: &mut TranscodeSession) {
    if session.paused {
        return;
    }
    let Some(process) = session.process.as_ref() else {
        return;
    };
    if signal_process(process.pid, libc::SIGSTOP).is_ok() {
        session.paused = true;
    }
}

fn resume_transcode(session: &mut TranscodeSession) {
    if !session.paused {
        return;
    }
    let Some(process) = session.process.as_ref() else {
        return;
    };
    if signal_process(process.pid, libc::SIGCONT).is_ok() {
        session.paused = false;
    }
}

fn signal_process(pid: u32, sig: i32) -> Result<(), std::io::Error> {
    let ret = unsafe { libc::kill(pid as i32, sig) };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

async fn segment_file_exists(
    storage: &StoragePaths,
    owner_id: Uuid,
    session_id: Uuid,
    variant_index: u32,
    segment_index: i32,
) -> bool {
    let path = storage
        .hls_variant_folder(&owner_id, &session_id, variant_index)
        .join(format!("seg_{segment_index}.m4s"));
    tokio::fs::metadata(path).await.is_ok()
}

fn segment_key(session_id: Uuid, variant_index: u32, segment_index: i32) -> String {
    format!("{session_id}:{variant_index}:{segment_index}")
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505")
    )
}
