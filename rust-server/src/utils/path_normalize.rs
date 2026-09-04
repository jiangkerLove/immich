use std::path::{Component, Path, PathBuf};

/// Approximate Node `path.normalize` for absolute import paths.
///
/// Preserves forward-slash style when the input uses `/` (typical Immich/DB paths),
/// so Windows hosts still match Linux-normalized library paths.
pub fn normalize_path(path: &str) -> String {
    let prefer_forward = path.contains('/') || !path.contains('\\');
    let mut normalized = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    let result = normalized.to_string_lossy().into_owned();
    if prefer_forward {
        result.replace('\\', "/")
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_path;
    use std::path::PathBuf;

    #[test]
    fn collapses_dot_segments() {
        assert_eq!(normalize_path("/data/./photos"), "/data/photos");
        assert_eq!(normalize_path("/data/foo/../photos"), "/data/photos");
    }

    #[cfg(windows)]
    #[test]
    fn preserves_windows_separator_style() {
        let expected = PathBuf::from(r"C:\data\photos")
            .to_string_lossy()
            .into_owned();
        assert_eq!(normalize_path(r"C:\data\.\photos"), expected);
    }
}
