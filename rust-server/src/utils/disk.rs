use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub struct DiskUsage {
    pub total: u64,
    pub used: u64,
    pub available: u64,
}

pub fn check_disk_usage(path: &Path) -> Option<DiskUsage> {
    let path = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };

    let output = Command::new("df")
        .args(["-Pk", path.to_string_lossy().as_ref()])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().nth(1)?;
    let mut fields = line.split_whitespace();
    let _filesystem = fields.next()?;
    let total_k = fields.next()?.parse::<u64>().ok()?;
    let used_k = fields.next()?.parse::<u64>().ok()?;
    let available_k = fields.next()?.parse::<u64>().ok()?;

    Some(DiskUsage {
        total: total_k * 1024,
        used: used_k * 1024,
        available: available_k * 1024,
    })
}
