use rand::RngCore;
use base64::{engine::general_purpose, Engine as _};
use sha2::{Digest, Sha256};

pub fn random_bytes_as_text(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);

    let base64_str = general_purpose::STANDARD.encode(&buf);
    // 过滤掉非单词字符
    base64_str.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect()
}

pub fn hash_sha256(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let result = hasher.finalize();
    general_purpose::STANDARD.encode(result)
}
