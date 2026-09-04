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
