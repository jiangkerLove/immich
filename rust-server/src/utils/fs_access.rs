use std::ffi::CString;
use std::path::Path;

/// Matches Node `fs.access(path, fs.constants.R_OK)`.
pub fn has_read_access(path: impl AsRef<Path>) -> bool {
    let Some(c_path) = path
        .as_ref()
        .to_str()
        .and_then(|value| CString::new(value).ok())
    else {
        return false;
    };
    unsafe { libc::access(c_path.as_ptr(), libc::R_OK) == 0 }
}

#[cfg(test)]
mod tests {
    use super::has_read_access;
    use std::os::unix::fs::PermissionsExt;

    fn is_root() -> bool {
        unsafe { libc::geteuid() == 0 }
    }

    #[test]
    fn reports_readable_and_missing_paths() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("photo.jpg");
        std::fs::write(&file, b"x").expect("write fixture");

        assert!(has_read_access(&file));
        assert!(has_read_access(dir.path()));
        assert!(!has_read_access(dir.path().join("missing.jpg")));
    }

    #[test]
    fn rejects_unreadable_files_when_not_root() {
        if is_root() {
            return;
        }

        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("secret.jpg");
        std::fs::write(&file, b"x").expect("write fixture");

        let mut permissions = std::fs::metadata(&file).expect("metadata").permissions();
        permissions.set_mode(0o000);
        std::fs::set_permissions(&file, permissions).expect("chmod");

        assert!(!has_read_access(&file));
    }
}
