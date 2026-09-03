use std::path::{Path, PathBuf};

use sysinfo::Disks;

#[derive(Debug, Clone, Copy)]
pub struct DiskUsage {
    pub total: u64,
    pub used: u64,
    pub available: u64,
}

/// Cross-platform disk usage for a library/media path.
/// Prefer sysinfo (works on Windows/Linux/macOS) over shelling out to `df`.
pub fn check_disk_usage(path: &Path) -> Option<DiskUsage> {
    let resolved = resolve_existing_path(path)?;
    let disks = Disks::new_with_refreshed_list();

    let mut best: Option<&sysinfo::Disk> = None;
    let mut best_len = 0usize;
    for disk in disks.list() {
        let mount = disk.mount_point();
        if path_is_under(&resolved, mount) {
            let len = mount.as_os_str().len();
            if len >= best_len {
                best_len = len;
                best = Some(disk);
            }
        }
    }

    let disk = best?;
    let total = disk.total_space();
    let available = disk.available_space();
    Some(DiskUsage {
        total,
        used: total.saturating_sub(available),
        available,
    })
}

fn resolve_existing_path(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return Some(path.to_path_buf());
    }
    path.parent().filter(|parent| parent.exists()).map(Path::to_path_buf)
}

fn path_is_under(path: &Path, mount: &Path) -> bool {
    if mount.as_os_str().is_empty() {
        return false;
    }
    path.starts_with(mount)
}

#[cfg(test)]
mod tests {
    use super::check_disk_usage;
    use std::env;

    #[test]
    fn reports_usage_for_temp_dir() {
        let dir = env::temp_dir();
        let usage = check_disk_usage(&dir).expect("disk usage for temp dir");
        assert!(usage.total > 0);
        assert!(usage.available <= usage.total);
        assert_eq!(usage.used, usage.total.saturating_sub(usage.available));
    }
}
