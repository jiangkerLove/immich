/// If a value was hex-encoded in the DB (e.g. `\xdeadbeef`), decode and return base64.
/// Otherwise treat bytes as raw and encode as base64.
pub fn hex_or_buffer_to_base64(encoded: &[u8]) -> String {
    use base64::Engine;
    if encoded.starts_with(b"\\x") {
        let hex_str = std::str::from_utf8(&encoded[2..]).unwrap_or("");
        if let Ok(bytes) = hex::decode(hex_str) {
            return base64::engine::general_purpose::STANDARD.encode(bytes);
        }
    }
    base64::engine::general_purpose::STANDARD.encode(encoded)
}

pub fn as_human_readable(bytes: u64, precision: usize) -> String {
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut magnitude = 0usize;
    let mut remainder = bytes as f64;

    while remainder >= 1024.0 && magnitude + 1 < UNITS.len() {
        magnitude += 1;
        remainder /= 1024.0;
    }

    let decimals = if magnitude == 0 { 0 } else { precision };
    format!("{remainder:.decimals$} {}", UNITS[magnitude])
}
