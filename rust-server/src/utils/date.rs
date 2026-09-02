use chrono::{DateTime, FixedOffset};

/// Extract a fixed-offset timezone label from an ISO datetime string.
///
/// Mirrors Node's `extractTimeZone`, which only returns fixed offsets such as
/// `UTC-7` rather than IANA timezone names.
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
