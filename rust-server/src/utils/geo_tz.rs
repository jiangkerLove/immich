use std::sync::OnceLock;

use tzf_rs::DefaultFinder;

static FINDER: OnceLock<DefaultFinder> = OnceLock::new();

fn finder() -> &'static DefaultFinder {
    FINDER.get_or_init(DefaultFinder::new)
}

/// Resolve an IANA timezone name from GPS coordinates.
///
/// Coordinates are passed in `(longitude, latitude)` order to match geo-tz/tzf-rs.
pub fn find_timezone(longitude: f64, latitude: f64) -> Option<String> {
    let timezone = finder().get_tz_name(longitude, latitude);
    if timezone.is_empty() {
        None
    } else {
        Some(timezone.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::find_timezone;

    #[test]
    fn resolves_los_angeles_from_coordinates() {
        let timezone = find_timezone(-118.2437, 34.0522).expect("timezone");
        assert_eq!(timezone, "America/Los_Angeles");
    }

    #[test]
    fn resolves_berlin_from_coordinates() {
        let timezone = find_timezone(13.4050, 52.5200).expect("timezone");
        assert_eq!(timezone, "Europe/Berlin");
    }
}
