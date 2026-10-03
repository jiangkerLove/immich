//! Supported media file extensions (mirrors server/src/utils/mime-types.ts).

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

fn has_extension(path: &str, extensions: &[&str]) -> bool {
    let lower = path.to_ascii_lowercase();
    extensions
        .iter()
        .any(|extension| lower.ends_with(extension))
}

#[cfg(test)]
mod tests {
    use super::{is_heif_image_path, is_possibly_animated_image_path};

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
}
