use serde_json::Value;

#[derive(Debug, Clone)]
pub struct SyncAck {
    pub ack_type: String,
    pub update_id: String,
    pub extra_id: Option<String>,
}

pub fn from_ack(ack: &str) -> SyncAck {
    let parts: Vec<&str> = ack.split('|').collect();
    SyncAck {
        ack_type: parts.first().unwrap_or(&"").to_string(),
        update_id: parts.get(1).unwrap_or(&"").to_string(),
        extra_id: parts.get(2).map(|s| s.to_string()),
    }
}

pub fn to_ack(ack: &SyncAck) -> String {
    match &ack.extra_id {
        Some(extra) => format!("{}|{}|{}", ack.ack_type, ack.update_id, extra),
        None => format!("{}|{}", ack.ack_type, ack.update_id),
    }
}

pub fn serialize(sync_type: &str, data: &Value, ids: &[&str], ack_type: Option<&str>) -> String {
    let ack = to_ack(&SyncAck {
        ack_type: ack_type.unwrap_or(sync_type).to_string(),
        update_id: ids[0].to_string(),
        extra_id: ids.get(1).map(|s| s.to_string()),
    });
    let line = serde_json::json!({
        "type": sync_type,
        "data": data,
        "ack": ack,
    });
    format!("{line}\n")
}
