use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;

#[derive(Debug, Clone, Default)]
pub struct VideoInterfaces {
    pub dri: Vec<String>,
    pub mali: bool,
}

pub fn detect_video_interfaces() -> VideoInterfaces {
    VideoInterfaces {
        dri: detect_dri_devices(),
        mali: detect_mali_opencl(),
    }
}

fn detect_dri_devices() -> Vec<String> {
    let dri_path = Path::new("/dev/dri");
    let Ok(entries) = std::fs::read_dir(dri_path) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with("renderD") || name.starts_with("card"))
        .collect()
}

fn detect_mali_opencl() -> bool {
    let icd = Path::new("/etc/OpenCL/vendors/mali.icd");
    let device = Path::new("/dev/mali0");
    if !icd.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        device
            .metadata()
            .map(|meta| meta.file_type().is_char_device())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        device.is_file()
    }
}

pub fn resolve_hw_device(interfaces: &VideoInterfaces, preferred: &str) -> Result<String, String> {
    if preferred == "auto" {
        let device = interfaces
            .dri
            .iter()
            .max_by(|left, right| left.cmp(right))
            .ok_or_else(|| {
                "No /dev/dri devices found. If using Docker, mount at least one /dev/dri device"
                    .to_string()
            })?;
        return Ok(format!("/dev/dri/{device}"));
    }

    let device_name = preferred.trim_start_matches("/dev/dri/");
    if !interfaces.dri.iter().any(|entry| entry == device_name) {
        return Err(format!(
            "Device '{device_name}' does not exist. If using Docker, make sure this device is mounted"
        ));
    }

    Ok(format!("/dev/dri/{device_name}"))
}
