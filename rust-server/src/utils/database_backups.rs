pub fn is_valid_database_backup_name(filename: &str) -> bool {
    let bytes = filename.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    let valid_char = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_';
    if !bytes.iter().all(|&b| valid_char(b)) {
        return false;
    }

    filename.ends_with(".sql") || filename.ends_with(".sql.gz")
}
