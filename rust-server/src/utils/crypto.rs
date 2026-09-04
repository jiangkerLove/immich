use base64::{Engine as _, engine::general_purpose};
use rand::RngCore;
use sha2::{Digest, Sha256};

pub fn random_bytes_as_text(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);

    general_purpose::STANDARD
        .encode(&buf)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}

/// Returns raw SHA-256 bytes, matching the Node.js `crypto.createHash('sha256').digest()`.
pub fn hash_sha256(value: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher.finalize().to_vec()
}

pub fn shared_link_login_token(id: &uuid::Uuid, password: &str) -> String {
    general_purpose::STANDARD.encode(hash_sha256(&format!("{id}-{password}")))
}
