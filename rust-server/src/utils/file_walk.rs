use std::path::{Path, PathBuf};

const DEFAULT_BATCH_SIZE: usize = 10_000;

pub fn walk_file_batches(
    roots: &[PathBuf],
    extensions: Option<&[String]>,
    batch_size: usize,
) -> Vec<Vec<String>> {
    let batch_size = if batch_size == 0 {
        DEFAULT_BATCH_SIZE
    } else {
        batch_size
    };

    let mut batches = Vec::new();
    let mut current = Vec::new();

    for root in roots {
        if !root.is_dir() {
            continue;
        }
        walk_dir(root, extensions, &mut current, batch_size, &mut batches);
    }

    if !current.is_empty() {
        batches.push(current);
    }

    batches
}

pub fn is_hidden_path(path: &Path) -> bool {
    path.components()
        .filter_map(|component| component.as_os_str().to_str())
        .any(|part| part.starts_with('.'))
}

fn walk_dir(
    dir: &Path,
    extensions: Option<&[String]>,
    current: &mut Vec<String>,
    batch_size: usize,
    batches: &mut Vec<Vec<String>>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(value) => value,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if is_hidden_path(&path) {
                continue;
            }
            walk_dir(&path, extensions, current, batch_size, batches);
            continue;
        }

        if !path.is_file() {
            continue;
        }

        if is_hidden_path(&path) {
            continue;
        }

        if let Some(exts) = extensions {
            if !matches_extension(&path, exts) {
                continue;
            }
        }

        let Some(path_str) = path.to_str().map(str::to_string) else {
            continue;
        };

        current.push(path_str);
        if current.len() >= batch_size {
            batches.push(std::mem::take(current));
        }
    }
}

fn matches_extension(path: &Path, extensions: &[String]) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower = file_name.to_ascii_lowercase();
    extensions
        .iter()
        .any(|ext| lower.ends_with(&ext.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_hidden_files_and_directories() {
        let root = std::env::temp_dir().join(format!("immich-file-walk-{}", uuid::Uuid::new_v4()));
        let visible_dir = root.join("photos");
        let hidden_dir = root.join(".hidden");
        let hidden_file = visible_dir.join(".secret.jpg");
        let visible_file = visible_dir.join("photo.jpg");
        std::fs::create_dir_all(&visible_dir).unwrap();
        std::fs::create_dir_all(&hidden_dir).unwrap();
        std::fs::write(&hidden_file, "x").unwrap();
        std::fs::write(&visible_file, "x").unwrap();
        std::fs::write(hidden_dir.join("nested.jpg"), "x").unwrap();

        let batches = walk_file_batches(&[root.clone()], None, 10);
        let paths: Vec<String> = batches.into_iter().flatten().collect();

        assert_eq!(paths, vec![visible_file.to_string_lossy().into_owned()]);

        let _ = std::fs::remove_dir_all(root);
    }
}
