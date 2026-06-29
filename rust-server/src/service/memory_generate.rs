use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::db::advisory_lock::{self, LOCK_MEMORY_CREATION};
use crate::models::db::memory::{self, MemoryCreateData};
use crate::models::db::system_metadata::{self, MemoriesState};
use crate::models::db::users::UserDb;

const DAYS: i64 = 3;
const MEMORY_TYPE_ON_THIS_DAY: &str = "on_this_day";

pub async fn run_memory_generate(pool: &PgPool) -> Result<(), String> {
    let ran = advisory_lock::run_with_try_lock(pool, LOCK_MEMORY_CREATION, || async {
        generate_memories(pool).await
    })
    .await
    .map_err(|err| err.to_string())?;

    if ran.is_none() {
        println!("MemoryGenerate: another instance holds the lock, skipping");
    }

    Ok(())
}

async fn generate_memories(pool: &PgPool) -> Result<(), sqlx::Error> {
    let users = UserDb::list_admin(pool, None, false).await?;
    let state = system_metadata::get_memories_state(pool).await?;

    let today = Utc::now().date_naive();
    let start = today - chrono::Duration::days(DAYS);
    let last_on_this_day = state
        .last_on_this_day_date
        .as_deref()
        .and_then(parse_day_start)
        .unwrap_or(start_and_time(start));

    for offset in 0..=(DAYS * 2) {
        let target = start + chrono::Duration::days(offset);
        let target_start = start_and_time(target);
        if last_on_this_day >= target_start {
            continue;
        }

        println!("Creating memories for {}", target_start.to_rfc3339());
        if let Err(err) = create_memories_for_day(pool, &users, target).await {
            eprintln!("Failed to create memories for {target}: {err}");
        }

        let next_state = MemoriesState {
            last_on_this_day_date: Some(target_start.to_rfc3339()),
        };
        system_metadata::set_memories_state(pool, &next_state).await?;
    }

    Ok(())
}

async fn create_memories_for_day(
    pool: &PgPool,
    users: &[crate::models::db::users::UserDb],
    target: NaiveDate,
) -> Result<(), sqlx::Error> {
    for user in users {
        create_on_this_day_memories(pool, &user.id, target).await?;
    }
    Ok(())
}

async fn create_on_this_day_memories(
    pool: &PgPool,
    owner_id: &Uuid,
    target: NaiveDate,
) -> Result<(), sqlx::Error> {
    let show_at = start_and_time(target);
    let hide_at = end_of_day(target);
    let groups = memory::get_assets_by_day_of_year(
        pool,
        &[*owner_id],
        target.month() as i32,
        target.day() as i32,
        target.year(),
    )
    .await?;

    for group in groups {
        if group.asset_ids.is_empty() {
            continue;
        }

        let memory_at = memory_at_for_year(target, group.year);
        memory::create(
            pool,
            &MemoryCreateData {
                owner_id: *owner_id,
                memory_type: MEMORY_TYPE_ON_THIS_DAY.to_string(),
                data: serde_json::json!({ "year": group.year }),
                is_saved: false,
                memory_at,
                seen_at: None,
                show_at: Some(show_at),
                hide_at: Some(hide_at),
            },
            &group.asset_ids,
        )
        .await?;
    }

    Ok(())
}

fn start_and_time(date: NaiveDate) -> chrono::DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("valid start of day"))
}

fn end_of_day(date: NaiveDate) -> chrono::DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_hms_opt(23, 59, 59).expect("valid end of day"))
}

fn memory_at_for_year(target: NaiveDate, year: i32) -> chrono::DateTime<Utc> {
    let date = NaiveDate::from_ymd_opt(year, target.month(), target.day())
        .unwrap_or(target);
    start_and_time(date)
}

fn parse_day_start(value: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
        .or_else(|| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .map(start_and_time)
        })
}
