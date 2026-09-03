#[cfg(unix)]
use std::ffi::CString;
use std::path::Path;

/// Matches Node `fs.access(path, fs.constants.R_OK)`.
pub fn has_read_access(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();

    #[cfg(unix)]
    {
        let Some(c_path) = path.to_str().and_then(|value| CString::new(value).ok()) else {
            return false;
        };
        unsafe { libc::access(c_path.as_ptr(), libc::R_OK) == 0 }
    }

    #[cfg(windows)]
    {
        if !path.exists() {
            return false;
        }
        if path.is_dir() {
            std::fs::read_dir(path).is_ok()
        } else {
            std::fs::File::open(path).is_ok()
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        path.exists()
    }
}

#[cfg(all(test, unix))]
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

#[cfg(all(test, windows))]
mod windows_tests {
    use super::has_read_access;

    #[test]
    fn reports_readable_and_missing_paths() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("photo.jpg");
        std::fs::write(&file, b"x").expect("write fixture");

        assert!(has_read_access(&file));
        assert!(has_read_access(dir.path()));
        assert!(!has_read_access(dir.path().join("missing.jpg")));
    }
}
