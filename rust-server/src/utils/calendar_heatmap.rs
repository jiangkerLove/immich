use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::models::db::assets;
use crate::models::response::response::ErrorResp;

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarHeatmapQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(rename = "type")]
    pub heatmap_type: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarHeatmapSeriesItem {
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarHeatmapResponse {
    pub from: String,
    pub to: String,
    pub series: Vec<CalendarHeatmapSeriesItem>,
    pub total_count: i64,
}

pub async fn build_calendar_heatmap(
    pool: &PgPool,
    owner_id: &Uuid,
    query: &CalendarHeatmapQuery,
) -> Result<CalendarHeatmapResponse, ErrorResp> {
    let to_date = parse_date(query.to.as_deref()).unwrap_or_else(|| Utc::now().date_naive());
    let from_date = parse_date(query.from.as_deref()).unwrap_or_else(|| {
        to_date - Duration::weeks(52) + Duration::days(1)
    });

    if from_date > to_date {
        return Err(ErrorResp::BadRequest("from must be before to".to_string()));
    }

    let taken_at = query
        .heatmap_type
        .as_deref()
        .map(|value| value.eq_ignore_ascii_case("taken"))
        .unwrap_or(false);

    let from_dt = naive_date_start(from_date);
    let to_exclusive = naive_date_start(to_date + Duration::days(1));

    let rows =
        assets::get_calendar_heatmap(pool, owner_id, from_dt, to_exclusive, taken_at).await?;

    let mut counts: HashMap<String, i64> = HashMap::new();
    for row in rows {
        counts.insert(format_date(row.date.date_naive()), row.count);
    }

    let mut series = Vec::new();
    let mut total_count = 0i64;
    let mut current = from_date;
    while current <= to_date {
        let key = format_date(current);
        let count = counts.get(&key).copied().unwrap_or(0);
        total_count += count;
        series.push(CalendarHeatmapSeriesItem { date: key, count });
        current += Duration::days(1);
    }

    Ok(CalendarHeatmapResponse {
        from: format_date(from_date),
        to: format_date(to_date),
        series,
        total_count,
    })
}

fn parse_date(value: Option<&str>) -> Option<NaiveDate> {
    value.and_then(|raw| {
        NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .ok()
            .or_else(|| DateTime::parse_from_rfc3339(raw).ok().map(|dt| dt.date_naive()))
    })
}

fn naive_date_start(date: NaiveDate) -> DateTime<Utc> {
    date.and_hms_opt(0, 0, 0)
        .unwrap()
        .and_local_timezone(Utc)
        .unwrap()
}

fn format_date(date: NaiveDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day())
}
