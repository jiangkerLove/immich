use base64::{Engine as _, engine::general_purpose};

pub fn decode_checksum(value: &str) -> Result<Vec<u8>, String> {
    if value.len() == 40 {
        hex::decode(value).map_err(|e| e.to_string())
    } else {
        general_purpose::STANDARD
            .decode(value)
            .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(value))
            .map_err(|e| e.to_string())
    }
}

/// Match TS `fromChecksum`: length 28 → base64, otherwise Node-style hex (never errors).
pub fn from_checksum(value: &str) -> Vec<u8> {
    if value.len() == 28 {
        return general_purpose::STANDARD
            .decode(value)
            .or_else(|_| general_purpose::URL_SAFE.decode(value))
            .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(value))
            .unwrap_or_default();
    }
    decode_node_hex(value)
}

fn decode_node_hex(value: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut nibble: Option<u8> = None;
    for ch in value.bytes() {
        let digit = match ch {
            b'0'..=b'9' => ch - b'0',
            b'a'..=b'f' => ch - b'a' + 10,
            b'A'..=b'F' => ch - b'A' + 10,
            _ => break,
        };
        if let Some(high) = nibble.take() {
            out.push((high << 4) | digit);
        } else {
            nibble = Some(digit);
        }
    }
    out
}

pub fn sha1_bytes(data: &[u8]) -> Vec<u8> {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

pub fn decode_share_key(key: &str) -> Result<Vec<u8>, String> {
    if key.len() == 100 {
        hex::decode(key).map_err(|e| e.to_string())
    } else {
        general_purpose::URL_SAFE_NO_PAD
            .decode(key)
            .or_else(|_| general_purpose::STANDARD.decode(key))
            .map_err(|e| e.to_string())
    }
}
