use chrono::{DateTime, FixedOffset, NaiveDate, Utc};

/// Extract a fixed-offset timezone label from an ISO datetime string.
///
/// Mirrors Node's `extractTimeZone`, which only returns fixed offsets such as
/// `UTC-7` rather than IANA timezone names.
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

fn fixed_offset_label(offset: &FixedOffset) -> String {
    let total_seconds = offset.local_minus_utc();
    if total_seconds == 0 {
        return "UTC".to_string();
    }

    let hours = total_seconds / 3600;
    let minutes = (total_seconds.abs() % 3600) / 60;
    if minutes == 0 {
        if hours > 0 {
            format!("UTC+{hours}")
        } else {
            format!("UTC{hours}")
        }
    } else {
        offset.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::extract_fixed_time_zone;

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
}
