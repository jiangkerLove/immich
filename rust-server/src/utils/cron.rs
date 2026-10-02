use std::str::FromStr;

use chrono::{DateTime, Local};
use cron::Schedule;
use serde_json::Value;

pub fn normalize_cron_expression(expression: &str) -> String {
    let trimmed = expression.trim();
    match trimmed.split_whitespace().count() {
        5 => format!("0 {trimmed}"),
        _ => trimmed.to_string(),
    }
}

pub fn nightly_tasks_cron_expression(config: &Value) -> String {
    let start_time = config
        .get("nightlyTasks")
        .and_then(|value| value.get("startTime"))
        .and_then(|value| value.as_str())
        .unwrap_or("00:00");

    let mut parts = start_time.split(':');
    let hour = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let minute = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    format!("0 {minute} {hour} * * *")
}

pub fn should_run_cron(expression: &str, now: DateTime<Local>, since: DateTime<Local>) -> bool {
    let normalized = normalize_cron_expression(expression);
    let Ok(schedule) = Schedule::from_str(&normalized) else {
        tracing::error!("cron: invalid expression '{expression}'");
        return false;
    };

    schedule.after(&since).take(1).any(|time| time <= now)
}

/// Error text from the `cron` npm package 4.4 `validateCronExpression`.
pub fn node_cron_error(expression: &str) -> Option<String> {
    parse_node_cron(expression).err()
}

fn parse_node_cron(expression: &str) -> Result<(), String> {
    let mut source = expression.to_ascii_lowercase();
    if let Some(preset) = cron_preset(&source) {
        source = preset.to_string();
    }
    source = replace_cron_aliases(&source)?;
    let units: Vec<&str> = source.split_whitespace().collect();
    if units.len() < 5 {
        return Err("Too few fields".to_string());
    }
    if units.len() > 6 {
        return Err("Too many fields".to_string());
    }
    let names = [
        "second",
        "minute",
        "hour",
        "dayOfMonth",
        "month",
        "dayOfWeek",
    ];
    let defaults = ["0", "*", "*", "*", "*", "*"];
    let bounds = [(0, 59), (0, 59), (0, 23), (1, 31), (1, 12), (0, 7)];
    for (index, name) in names.iter().enumerate() {
        let field_index = index as isize - (6 - units.len() as isize);
        let field = if field_index < 0 {
            defaults[index]
        } else {
            units[field_index as usize]
        };
        parse_cron_field(field, name, bounds[index])?;
    }
    Ok(())
}

fn cron_preset(source: &str) -> Option<&'static str> {
    Some(match source {
        "@yearly" => "0 0 0 1 1 *",
        "@monthly" => "0 0 0 1 * *",
        "@weekly" => "0 0 0 * * 0",
        "@daily" => "0 0 0 * * *",
        "@hourly" => "0 0 * * * *",
        "@minutely" => "0 * * * * *",
        "@secondly" => "* * * * * *",
        "@weekdays" => "0 0 0 * * 1-5",
        "@weekends" => "0 0 0 * * 0,6",
        _ => return None,
    })
}

fn replace_cron_aliases(source: &str) -> Result<String, String> {
    let alias = regex::Regex::new(r"[a-z]{1,3}").unwrap();
    let mut out = String::new();
    let mut last = 0;
    for found in alias.find_iter(source) {
        out.push_str(&source[last..found.start()]);
        let token = found.as_str();
        let Some(value) = cron_alias(token) else {
            return Err(format!("Unknown alias: {token}"));
        };
        out.push_str(&value.to_string());
        last = found.end();
    }
    out.push_str(&source[last..]);
    Ok(out)
}

fn cron_alias(token: &str) -> Option<u8> {
    Some(match token {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        "sun" => 0,
        "mon" => 1,
        "tue" => 2,
        "wed" => 3,
        "thu" => 4,
        "fri" => 5,
        "sat" => 6,
        _ => return None,
    })
}

fn parse_cron_field(value: &str, unit: &str, bounds: (i32, i32)) -> Result<(), String> {
    let (low, high) = bounds;
    for field in value.split(',') {
        if let Some(index) = field.find('*') {
            if index != 0 {
                return Err(format!(
                    "Field ({field}) has an invalid wildcard expression"
                ));
            }
        }
    }
    let expanded = value.replace('*', &format!("{low}-{high}"));
    let range = regex::Regex::new(r"^(\d+)(?:-(\d+))?(?:/(\d+))?$").unwrap();
    for piece in expanded.split(',') {
        let Some(caps) = range.captures(piece) else {
            return Err(format!("Field ({unit}) cannot be parsed"));
        };
        let lower: i32 = caps[1].parse().unwrap_or(0);
        let upper = caps
            .get(2)
            .and_then(|item| item.as_str().parse::<i32>().ok());
        let step = caps
            .get(3)
            .and_then(|item| item.as_str().parse::<i32>().ok())
            .unwrap_or(1);
        if step == 0 {
            return Err(format!("Field ({unit}) has a step of zero"));
        }
        if upper.is_some_and(|upper| lower > upper) {
            return Err(format!("Field ({unit}) has an invalid range"));
        }
        let out_of_range = lower < low
            || upper.is_some_and(|upper| upper > high)
            || (upper.is_none() && lower > high);
        if out_of_range {
            return Err(format!("Field value ({expanded}) is out of range"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::node_cron_error;

    #[test]
    fn node_cron_messages_match_package_4_4() {
        assert!(node_cron_error("0 2 * * *").is_none());
        assert_eq!(
            node_cron_error("not a cron").as_deref(),
            Some("Unknown alias: not")
        );
        assert_eq!(node_cron_error("*").as_deref(), Some("Too few fields"));
        assert_eq!(
            node_cron_error("60 * * * *").as_deref(),
            Some("Field value (60) is out of range")
        );
    }
}
