use chrono::{DateTime, FixedOffset, NaiveDate, Utc};

/// Extract a fixed-offset timezone label from an ISO datetime string.
///
/// Mirrors Node's `extractTimeZone`, which only returns fixed offsets such as
/// `UTC-7` / `UTC+5:30` (Luxon fixed-zone `.name`) rather than IANA names.
/// Parse EXIF `dateTimeOriginal` the way the TypeScript server stores it.
///
/// Full timestamps stay timezone-aware. A date-only `YYYY-MM-DD` is UTC midnight,
/// matching the memories lane parser.
pub fn parse_exif_datetime(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(datetime) = DateTime::parse_from_rfc3339(value) {
        return Some(datetime.with_timezone(&Utc));
    }
    if let Ok(datetime) = value.parse::<DateTime<Utc>>() {
        return Some(datetime);
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|naive| naive.and_utc())
}

pub fn extract_fixed_time_zone(date_time_original: &str) -> Option<String> {
    let datetime = DateTime::parse_from_rfc3339(date_time_original).ok()?;
    Some(fixed_offset_label(datetime.offset()))
}

/// Parse Luxon-style fixed zone names (`UTC`, `UTC-7`, `UTC+5:30`) or `±HH:MM`.
pub fn parse_fixed_time_zone(time_zone: &str) -> Option<FixedOffset> {
    if time_zone == "UTC" {
        return FixedOffset::east_opt(0);
    }

    if let Some(rest) = time_zone.strip_prefix("UTC") {
        return parse_luxon_offset(rest);
    }

    time_zone
        .parse::<FixedOffset>()
        .ok()
        .or_else(|| parse_luxon_offset(time_zone))
}

fn parse_luxon_offset(value: &str) -> Option<FixedOffset> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let (sign, rest) = match value.chars().next()? {
        '+' => (1, &value[1..]),
        '-' => (-1, &value[1..]),
        _ => return None,
    };

    let (hours, minutes) = if let Some((h, m)) = rest.split_once(':') {
        (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?)
    } else if rest.len() == 4 && rest.chars().all(|c| c.is_ascii_digit()) {
        (
            rest[..2].parse::<i32>().ok()?,
            rest[2..].parse::<i32>().ok()?,
        )
    } else {
        (rest.parse::<i32>().ok()?, 0)
    };

    FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
}

fn fixed_offset_label(offset: &FixedOffset) -> String {
    let total_seconds = offset.local_minus_utc();
    if total_seconds == 0 {
        return "UTC".to_string();
    }

    let sign = if total_seconds > 0 { "+" } else { "-" };
    let abs = total_seconds.unsigned_abs();
    let hours = abs / 3600;
    let minutes = (abs % 3600) / 60;
    if minutes == 0 {
        format!("UTC{sign}{hours}")
    } else {
        format!("UTC{sign}{hours}:{minutes:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_fixed_time_zone, parse_fixed_time_zone};

    #[test]
    fn extracts_whole_hour_fixed_offsets() {
        assert_eq!(
            extract_fixed_time_zone("2023-11-19T18:11:00.000-07:00").as_deref(),
            Some("UTC-7")
        );
        assert_eq!(
            extract_fixed_time_zone("2023-11-19T18:11:00.000+02:00").as_deref(),
            Some("UTC+2")
        );
    }

    #[test]
    fn extracts_half_hour_as_luxon_name() {
        assert_eq!(
            extract_fixed_time_zone("2023-11-19T18:11:00.000+05:30").as_deref(),
            Some("UTC+5:30")
        );
    }

    #[test]
    fn extracts_utc_for_z_suffix() {
        assert_eq!(
            extract_fixed_time_zone("2023-11-19T18:11:00.000Z").as_deref(),
            Some("UTC")
        );
    }

    #[test]
    fn ignores_timezone_less_datetime_strings() {
        assert!(extract_fixed_time_zone("2023-11-19T18:11:00").is_none());
    }

    #[test]
    fn parses_luxon_fixed_zone_names() {
        assert_eq!(
            parse_fixed_time_zone("UTC-7").map(|o| o.local_minus_utc()),
            Some(-7 * 3600)
        );
        assert_eq!(
            parse_fixed_time_zone("UTC+5:30").map(|o| o.local_minus_utc()),
            Some(5 * 3600 + 30 * 60)
        );
        assert_eq!(
            parse_fixed_time_zone("UTC").map(|o| o.local_minus_utc()),
            Some(0)
        );
    }
}
