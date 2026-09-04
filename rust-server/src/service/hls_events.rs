//! HLS cross-process coordination via Redis pub/sub.
//!
//! Mirrors TypeScript Socket.IO `serverSideEmit` events used between the API
//! process (`HlsService`) and the microservices process (`TranscodingService`).
//! Combined single-process mode should not publish these — use in-process calls.

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const HLS_SESSION_REQUEST: &str = "immich:server:HlsSessionRequest";
pub const HLS_SESSION_RESULT: &str = "immich:server:HlsSessionResult";
pub const HLS_SESSION_END: &str = "immich:server:HlsSessionEnd";
pub const HLS_HEARTBEAT: &str = "immich:server:HlsHeartbeat";
pub const HLS_SEGMENT_REQUEST: &str = "immich:server:HlsSegmentRequest";
pub const HLS_SEGMENT_RESULT: &str = "immich:server:HlsSegmentResult";

pub const HLS_CHANNELS: &[&str] = &[
    HLS_SESSION_REQUEST,
    HLS_SESSION_RESULT,
    HLS_SESSION_END,
    HLS_HEARTBEAT,
    HLS_SEGMENT_REQUEST,
    HLS_SEGMENT_RESULT,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlsSessionRequestMsg {
    pub session_id: Uuid,
    pub asset_id: Uuid,
    pub owner_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlsSessionResultMsg {
    pub session_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlsSessionEndMsg {
    pub session_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlsHeartbeatMsg {
    pub session_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_index: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlsSegmentRequestMsg {
    pub session_id: Uuid,
    pub asset_id: Uuid,
    pub variant_index: u32,
    pub segment_index: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlsSegmentResultMsg {
    pub session_id: Uuid,
    pub variant_index: u32,
    pub segment_index: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

async fn publish_json<T: Serialize>(redis_url: &str, channel: &str, message: &T) {
    let payload = match serde_json::to_string(message) {
        Ok(value) => value,
        Err(err) => {
            tracing::error!("hls events: serialize {channel} failed: {err}");
            return;
        }
    };

    let start = std::time::Instant::now();
    let result = async {
        let client = redis::Client::open(redis_url).map_err(|err| err.to_string())?;
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|err| err.to_string())?;
        conn.publish::<_, _, ()>(channel, payload)
            .await
            .map_err(|err| err.to_string())
    }
    .await;

    crate::utils::telemetry::record_redis_command(
        "publish",
        start.elapsed().as_secs_f64() * 1000.0,
        result.is_ok(),
    );

    if let Err(err) = result {
        tracing::error!("hls events: publish {channel} failed: {err}");
    }
}

pub async fn publish_session_request(redis_url: &str, msg: &HlsSessionRequestMsg) {
    publish_json(redis_url, HLS_SESSION_REQUEST, msg).await;
}

pub async fn publish_session_result(redis_url: &str, msg: &HlsSessionResultMsg) {
    publish_json(redis_url, HLS_SESSION_RESULT, msg).await;
}

pub async fn publish_session_end(redis_url: &str, msg: &HlsSessionEndMsg) {
    publish_json(redis_url, HLS_SESSION_END, msg).await;
}

pub async fn publish_heartbeat(redis_url: &str, msg: &HlsHeartbeatMsg) {
    publish_json(redis_url, HLS_HEARTBEAT, msg).await;
}

pub async fn publish_segment_request(redis_url: &str, msg: &HlsSegmentRequestMsg) {
    publish_json(redis_url, HLS_SEGMENT_REQUEST, msg).await;
}

pub async fn publish_segment_result(redis_url: &str, msg: &HlsSegmentResultMsg) {
    publish_json(redis_url, HLS_SEGMENT_RESULT, msg).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_request_roundtrip() {
        let msg = HlsSessionRequestMsg {
            session_id: Uuid::nil(),
            asset_id: Uuid::nil(),
            owner_id: Uuid::nil(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("sessionId"));
        let parsed: HlsSessionRequestMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, Uuid::nil());
    }

    #[test]
    fn segment_result_roundtrip() {
        let msg = HlsSegmentResultMsg {
            session_id: Uuid::nil(),
            variant_index: 1,
            segment_index: 2,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: HlsSegmentResultMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.variant_index, 1);
        assert_eq!(parsed.segment_index, 2);
    }
}
