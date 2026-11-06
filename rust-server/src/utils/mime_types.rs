//! Supported media file extensions (mirrors server/src/utils/mime-types.ts).

use std::path::Path;

const IMAGE_EXTENSIONS: &[&str] = &[
    ".3fr", ".ari", ".arw", ".avif", ".bmp", ".cap", ".cin", ".cr2", ".cr3", ".crw", ".dcr",
    ".dng", ".erf", ".fff", ".gif", ".heic", ".heif", ".hif", ".iiq", ".insp", ".jp2", ".jpe",
    ".jpeg", ".jpg", ".jxl", ".k25", ".kdc", ".mrw", ".mpo", ".nef", ".nrw", ".orf", ".ori",
    ".pef", ".png", ".psd", ".raf", ".raw", ".rw2", ".rwl", ".sr2", ".srf", ".srw", ".svg", ".tif",
    ".tiff", ".webp", ".x3f",
];

const VIDEO_EXTENSIONS: &[&str] = &[
    ".3gp", ".3gpp", ".avi", ".flv", ".insv", ".m2t", ".m2ts", ".m4v", ".mkv", ".mov", ".mp4",
    ".mpe", ".mpeg", ".mpg", ".mts", ".mxf", ".ts", ".vob", ".webm", ".wmv",
];

const SIDECAR_EXTENSIONS: &[&str] = &[".xmp"];
const HEIF_IMAGE_EXTENSIONS: &[&str] = &[".avif", ".heic", ".heif", ".hif"];
const POSSIBLY_ANIMATED_IMAGE_EXTENSIONS: &[&str] =
    &[".avif", ".gif", ".heic", ".heif", ".jxl", ".png", ".webp"];

/// First Content-Type for each extension (`mimeTypes.lookup`).
const CONTENT_TYPES: &[(&str, &str)] = &[
    (".3fr", "image/3fr"),
    (".ari", "image/ari"),
    (".arw", "image/arw"),
    (".avif", "image/avif"),
    (".bmp", "image/bmp"),
    (".cap", "image/cap"),
    (".cin", "image/cin"),
    (".cr2", "image/cr2"),
    (".cr3", "image/cr3"),
    (".crw", "image/crw"),
    (".dcr", "image/dcr"),
    (".dng", "image/dng"),
    (".erf", "image/erf"),
    (".fff", "image/fff"),
    (".gif", "image/gif"),
    (".heic", "image/heic"),
    (".heif", "image/heif"),
    (".hif", "image/hif"),
    (".iiq", "image/iiq"),
    (".insp", "image/jpeg"),
    (".jp2", "image/jp2"),
    (".jpe", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".jpg", "image/jpeg"),
    (".jxl", "image/jxl"),
    (".k25", "image/k25"),
    (".kdc", "image/kdc"),
    (".mrw", "image/mrw"),
    (".mpo", "image/jpeg"),
    (".nef", "image/nef"),
    (".nrw", "image/nrw"),
    (".orf", "image/orf"),
    (".ori", "image/ori"),
    (".pef", "image/pef"),
    (".png", "image/png"),
    (".psd", "image/psd"),
    (".raf", "image/raf"),
    (".raw", "image/raw"),
    (".rw2", "image/rw2"),
    (".rwl", "image/rwl"),
    (".sr2", "image/sr2"),
    (".srf", "image/srf"),
    (".srw", "image/srw"),
    (".svg", "image/svg"),
    (".tif", "image/tiff"),
    (".tiff", "image/tiff"),
    (".webp", "image/webp"),
    (".x3f", "image/x3f"),
    (".3gp", "video/3gpp"),
    (".3gpp", "video/3gpp"),
    (".avi", "video/avi"),
    (".flv", "video/x-flv"),
    (".insv", "video/mp4"),
    (".m2t", "video/mp2t"),
    (".m2ts", "video/mp2t"),
    (".m4v", "video/x-m4v"),
    (".mkv", "video/x-matroska"),
    (".mov", "video/quicktime"),
    (".mp4", "video/mp4"),
    (".mpe", "video/mpeg"),
    (".mpeg", "video/mpeg"),
    (".mpg", "video/mpeg"),
    (".mts", "video/mp2t"),
    (".mxf", "application/mxf"),
    (".ts", "video/mp2t"),
    (".vob", "video/mpeg"),
    (".webm", "video/webm"),
    (".wmv", "video/x-ms-wmv"),
    (".xmp", "application/xml"),
];

fn to_vec(extensions: &[&str]) -> Vec<String> {
    extensions.iter().map(|ext| (*ext).to_string()).collect()
}

pub fn supported_image_extensions() -> Vec<String> {
    to_vec(IMAGE_EXTENSIONS)
}

pub fn supported_video_extensions() -> Vec<String> {
    to_vec(VIDEO_EXTENSIONS)
}

pub fn supported_sidecar_extensions() -> Vec<String> {
    to_vec(SIDECAR_EXTENSIONS)
}

pub fn supported_file_extensions() -> Vec<String> {
    let mut extensions = supported_image_extensions();
    extensions.extend(supported_video_extensions());
    extensions
}

pub fn is_video_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    supported_video_extensions()
        .iter()
        .any(|ext| lower.ends_with(ext))
}

pub fn is_heif_image_path(path: &str) -> bool {
    has_extension(path, HEIF_IMAGE_EXTENSIONS)
}

pub fn is_possibly_animated_image_path(path: &str) -> bool {
    has_extension(path, POSSIBLY_ANIMATED_IMAGE_EXTENSIONS)
}

pub fn is_supported_media_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    supported_file_extensions()
        .iter()
        .any(|ext| lower.ends_with(ext))
}

pub fn is_asset_path(path: &str) -> bool {
    is_supported_media_path(path)
}

pub fn is_sidecar_path(path: &str) -> bool {
    has_extension(path, SIDECAR_EXTENSIONS)
}

/// Match TS mimeTypes.assetType: image → IMAGE, video/mxf → VIDEO, else OTHER.
pub fn asset_type_from_path(path: &str) -> &'static str {
    let content_type = lookup(path);
    if content_type.starts_with("image/") {
        "IMAGE"
    } else if content_type == "application/mxf" || content_type.starts_with("video/") {
        "VIDEO"
    } else {
        "OTHER"
    }
}

/// `mimeTypes.lookup` — first mapped type, else `application/octet-stream`.
pub fn lookup(path: &str) -> String {
    let ext = filename_extension(path);
    CONTENT_TYPES
        .iter()
        .find(|(key, _)| *key == ext)
        .map(|(_, mime)| (*mime).to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

fn filename_extension(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.starts_with('.') && !name[1..].contains('.') {
        return name;
    }
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|ext| format!(".{}", ext.to_ascii_lowercase()))
        .unwrap_or_default()
}

fn has_extension(path: &str, extensions: &[&str]) -> bool {
    let lower = path.to_ascii_lowercase();
    extensions
        .iter()
        .any(|extension| lower.ends_with(extension))
}

#[cfg(test)]
mod tests {
    use super::{is_heif_image_path, is_possibly_animated_image_path, lookup};

    #[test]
    fn animated_and_heif_extensions_match_official() {
        for path in [
            "a.avif", "a.GIF", "a.heic", "a.heif", "a.jxl", "a.PNG", "a.webp",
        ] {
            assert!(is_possibly_animated_image_path(path), "{path}");
        }
        assert!(!is_possibly_animated_image_path("a.cr3"));
        assert!(!is_possibly_animated_image_path("a.jpg"));

        for path in ["a.AVIF", "a.heic", "a.heif", "a.hif"] {
            assert!(is_heif_image_path(path), "{path}");
        }
        assert!(!is_heif_image_path("a.jpg"));
    }

    #[test]
    fn raw_lookup_uses_image_slash_types() {
        assert_eq!(lookup("photo.dng"), "image/dng");
        assert_eq!(lookup("photo.CR2"), "image/cr2");
        assert_eq!(lookup("clip.mp4"), "video/mp4");
        assert_eq!(lookup("unknown.bin"), "application/octet-stream");
    }
}
