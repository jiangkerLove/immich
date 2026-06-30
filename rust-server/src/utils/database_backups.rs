pub fn find_database_backup_version(filename: &str) -> Option<String> {
    let start = filename.find("-v")?;
    let rest = &filename[start + 2..];
    let end = rest.find('-')?;
    let version = rest.get(..end)?;
    if version.is_empty() {
        return None;
    }
    Some(version.to_string())
}

pub fn is_legacy_pg_cluster_dump(filename: &str) -> bool {
    let Some(version) = find_database_backup_version(filename) else {
        return false;
    };
    let Ok(parsed) = semver::Version::parse(&version) else {
        return false;
    };
    semver::VersionReq::parse("<=2.4.0")
        .map(|req| req.matches(&parsed))
        .unwrap_or(false)
}

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

pub fn is_valid_database_routine_backup_name(filename: &str) -> bool {
    if filename.starts_with("uploaded-") {
        return false;
    }
    let old_style = regex_old_backup_style(filename);
    let new_style = regex_new_backup_style(filename);
    old_style || new_style
}

pub fn is_failed_database_backup_name(filename: &str) -> bool {
    filename.starts_with("immich-db-backup-") && filename.ends_with(".sql.gz.tmp")
}

fn regex_old_backup_style(filename: &str) -> bool {
    // immich-db-backup-<digits>.sql.gz
    filename.starts_with("immich-db-backup-")
        && filename.ends_with(".sql.gz")
        && !filename.ends_with(".sql.gz.tmp")
        && filename
            .strip_prefix("immich-db-backup-")
            .and_then(|s| s.strip_suffix(".sql.gz"))
            .is_some_and(|middle| !middle.is_empty() && middle.chars().all(|c| c.is_ascii_digit()))
}

fn regex_new_backup_style(filename: &str) -> bool {
    // immich-db-backup-20250729T114018-v1.136.0-pg14.17.sql.gz
    if !filename.starts_with("immich-db-backup-") || !filename.ends_with(".sql.gz") {
        return false;
    }
    let middle = filename
        .strip_prefix("immich-db-backup-")
        .and_then(|s| s.strip_suffix(".sql.gz"))
        .unwrap_or("");
    let Some((timestamp, rest)) = middle.split_once("-v") else {
        return false;
    };
    timestamp.len() == 15
        && timestamp.as_bytes()[8] == b'T'
        && timestamp[..8].chars().all(|c| c.is_ascii_digit())
        && timestamp[9..].chars().all(|c| c.is_ascii_digit())
        && rest.contains("-pg")
}
