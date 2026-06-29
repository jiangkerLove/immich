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
    let hour = parts.next().and_then(|value| value.parse::<u32>().ok()).unwrap_or(0);
    let minute = parts.next().and_then(|value| value.parse::<u32>().ok()).unwrap_or(0);
    format!("0 {minute} {hour} * * *")
}

pub fn should_run_cron(
    expression: &str,
    now: DateTime<Local>,
    since: DateTime<Local>,
) -> bool {
    let normalized = normalize_cron_expression(expression);
    let Ok(schedule) = Schedule::from_str(&normalized) else {
        eprintln!("cron: invalid expression '{expression}'");
        return false;
    };

    schedule.after(&since).take(1).any(|time| time <= now)
}
