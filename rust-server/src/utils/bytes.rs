pub fn as_human_readable(bytes: u64, precision: usize) -> String {
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut magnitude = 0usize;
    let mut remainder = bytes as f64;

    while remainder >= 1024.0 && magnitude + 1 < UNITS.len() {
        magnitude += 1;
        remainder /= 1024.0;
    }

    let decimals = if magnitude == 0 { 0 } else { precision };
    format!("{remainder:.decimals$} {}", UNITS[magnitude])
}
