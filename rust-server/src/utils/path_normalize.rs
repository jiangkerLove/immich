use std::path::{Component, Path, PathBuf};

/// Approximate Node `path.normalize` for absolute import paths.
pub fn normalize_path(path: &str) -> String {
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
    normalized.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::normalize_path;

    #[test]
    fn collapses_dot_segments() {
        assert_eq!(normalize_path("/data/./photos"), "/data/photos");
        assert_eq!(normalize_path("/data/foo/../photos"), "/data/photos");
    }
}
