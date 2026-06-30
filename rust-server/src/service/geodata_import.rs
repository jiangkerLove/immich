use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Instant;

use chrono::NaiveDate;
use futures_util::stream::{FuturesUnordered, StreamExt};
use serde::Deserialize;
use sqlx::{PgPool, QueryBuilder};
use tokio::sync::mpsc;
use tokio::task::spawn_blocking;

use crate::models::db::system_metadata::{
    self, ReverseGeocodingState,
};
use crate::models::dto::env::EnvDto;
use crate::service::database_bootstrap::LOCK_GEODATA_IMPORT;
use crate::service::job::JobService;
use crate::utils::geodata_paths::{self, CITIES_FILE, GeodataPaths};

const BATCH_SIZE: usize = 5000;
const MAX_IN_FLIGHT: usize = 9;

pub async fn init(pool: &PgPool, env: &EnvDto, jobs: &JobService) -> Result<(), String> {
    let paths = geodata_paths::resolve_geodata_paths(env);
    if !geodata_paths::geodata_dir_exists(&paths) {
        println!(
            "geodata import: geodata bundle not found at {} (reverse geocoding may be unavailable)",
            paths.date_file.display()
        );
        return Ok(());
    }

    let geodata_date = tokio::fs::read_to_string(&paths.date_file)
        .await
        .map_err(|err| format!("failed to read {}: {err}", paths.date_file.display()))?;
    let geodata_date = geodata_date.trim().to_string();

    let state = system_metadata::get_reverse_geocoding_state(pool)
        .await
        .map_err(|err| err.to_string())?;
    if state.last_update.as_deref() == Some(geodata_date.as_str()) {
        println!("geodata import: already up to date ({geodata_date})");
        return Ok(());
    }

    if let Some(missing) = geodata_paths::missing_geodata_file(&paths) {
        return Err(format!("Geodata file {} not found", missing.display()));
    }

    let mut lock_conn = pool
        .acquire()
        .await
        .map_err(|err| err.to_string())?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(LOCK_GEODATA_IMPORT)
        .execute(&mut *lock_conn)
        .await
        .map_err(|err| err.to_string())?;

    let import_result = run_import(pool, &paths, jobs).await;

    let _: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(LOCK_GEODATA_IMPORT)
        .fetch_one(&mut *lock_conn)
        .await
        .map_err(|err| err.to_string())?;

    import_result?;

    let next_state = ReverseGeocodingState {
        last_update: Some(geodata_date),
        last_import_file_name: Some(CITIES_FILE.to_string()),
    };
    system_metadata::set_reverse_geocoding_state(pool, &next_state)
        .await
        .map_err(|err| err.to_string())?;

    println!("geodata import: completed");
    Ok(())
}

async fn run_import(pool: &PgPool, paths: &GeodataPaths, jobs: &JobService) -> Result<(), String> {
    if let Err(err) = jobs.pause_metadata_extraction().await {
        eprintln!("geodata import: failed to pause metadata extraction: {err}");
    }

    let result = async {
        let (admin1, admin2) = tokio::try_join!(
            load_admin_map(&paths.admin1),
            load_admin_map(&paths.admin2),
        )?;

        import_geodata_places(pool, paths, &admin1, &admin2).await?;
        import_naturalearth_countries(pool, &paths.natural_earth_countries).await?;
        Ok(())
    }
    .await;

    if let Err(err) = jobs.resume_metadata_extraction().await {
        eprintln!("geodata import: failed to resume metadata extraction: {err}");
    }

    result
}

async fn load_admin_map(path: &Path) -> Result<HashMap<String, String>, String> {
    let path = path.to_path_buf();
    spawn_blocking(move || {
        let file = std::fs::File::open(&path)
            .map_err(|err| format!("Geodata file {} not found: {err}", path.display()))?;
        let reader = BufReader::new(file);
        let mut admin_map = HashMap::new();
        for line in reader.lines() {
            let line = line.map_err(|err| err.to_string())?;
            let mut parts = line.split('\t');
            let key = parts.next().unwrap_or_default().to_string();
            let value = parts.next().unwrap_or_default().to_string();
            if !key.is_empty() {
                admin_map.insert(key, value);
            }
        }
        Ok(admin_map)
    })
    .await
    .map_err(|err| err.to_string())?
}

async fn import_geodata_places(
    pool: &PgPool,
    paths: &GeodataPaths,
    admin1: &HashMap<String, String>,
    admin2: &HashMap<String, String>,
) -> Result<(), String> {
    sqlx::query("DROP TABLE IF EXISTS geodata_places_tmp")
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;

    let mut tx = pool.begin().await.map_err(|err| err.to_string())?;
    sqlx::query(
        r#"
        CREATE TABLE geodata_places_tmp (
            LIKE geodata_places INCLUDING ALL EXCLUDING INDEXES
        )
        "#,
    )
    .execute(&mut *tx)
    .await
    .map_err(|err| err.to_string())?;
    sqlx::query("DROP TABLE IF EXISTS geodata_places")
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    sqlx::query("ALTER TABLE geodata_places_tmp RENAME TO geodata_places")
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    tx.commit().await.map_err(|err| err.to_string())?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS "IDX_geodata_gist_earthcoord"
        ON geodata_places
        USING gist (ll_to_earth_public(latitude, longitude))
        "#,
    )
    .execute(pool)
    .await
    .map_err(|err| err.to_string())?;

    load_cities500(pool, &paths.cities500, admin1, admin2).await?;
    create_geodata_indices(pool).await?;
    Ok(())
}

#[derive(Clone)]
struct GeodataRecord {
    id: i32,
    name: String,
    latitude: f64,
    longitude: f64,
    country_code: String,
    admin1_code: Option<String>,
    admin2_code: Option<String>,
    modification_date: NaiveDate,
    admin1_name: Option<String>,
    admin2_name: Option<String>,
    alternate_names: Option<String>,
}

async fn load_cities500(
    pool: &PgPool,
    path: &Path,
    admin1: &HashMap<String, String>,
    admin2: &HashMap<String, String>,
) -> Result<(), String> {
    println!("geodata import: starting cities500 import");
    let start = Instant::now();
    let path = path.to_path_buf();
    let admin1 = admin1.clone();
    let admin2 = admin2.clone();

    let (tx, mut rx) = mpsc::channel(MAX_IN_FLIGHT);
    let read_handle = spawn_blocking(move || -> Result<usize, String> {
        let file = std::fs::File::open(&path)
            .map_err(|err| format!("Geodata file {} not found: {err}", path.display()))?;
        let reader = BufReader::new(file);
        let mut batch = Vec::with_capacity(BATCH_SIZE);
        let mut count = 0usize;

        for line in reader.lines() {
            let line = line.map_err(|err| err.to_string())?;
            if let Some(record) = parse_cities500_line(&line, &admin1, &admin2)? {
                batch.push(record);
                count += 1;
                if batch.len() >= BATCH_SIZE {
                    if tx.blocking_send(batch).is_err() {
                        return Ok(count);
                    }
                    batch = Vec::with_capacity(BATCH_SIZE);
                }
            }
        }

        if !batch.is_empty() {
            let _ = tx.blocking_send(batch);
        }
        Ok(count)
    });

    let pool = pool.clone();
    let mut futures = FuturesUnordered::new();
    let mut imported = 0usize;

    while let Some(batch) = rx.recv().await {
        let batch_len = batch.len();
        let pool = pool.clone();
        futures.push(async move { insert_geodata_batch(&pool, &batch).await });

        if futures.len() >= MAX_IN_FLIGHT {
            if let Some(result) = futures.next().await {
                result?;
            }
        }

        imported += batch_len;
        if imported % 10_000 == 0 {
            println!("geodata import: {imported} geodata records imported");
        }
    }

    while let Some(result) = futures.next().await {
        result?;
    }

    let count = read_handle
        .await
        .map_err(|err| err.to_string())??;

    let duration = start.elapsed().as_secs_f64();
    let records_per_second = if duration > 0.0 {
        (count as f64 / duration).round() as u64
    } else {
        count as u64
    };
    println!(
        "geodata import: successfully imported {count} geodata records in {duration:.2}s ({records_per_second} records/second)"
    );
    Ok(())
}

fn parse_cities500_line(
    line: &str,
    admin1: &HashMap<String, String>,
    admin2: &HashMap<String, String>,
) -> Result<Option<GeodataRecord>, String> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 19 {
        return Ok(None);
    }

    if (fields[7] == "PPLX" && fields[8] != "AU") || fields[7] == "PPLH" {
        return Ok(None);
    }

    let id = fields[0]
        .parse::<i32>()
        .map_err(|err| format!("invalid geodata id {}: {err}", fields[0]))?;
    let latitude = fields[4]
        .parse::<f64>()
        .map_err(|err| format!("invalid latitude {}: {err}", fields[4]))?;
    let longitude = fields[5]
        .parse::<f64>()
        .map_err(|err| format!("invalid longitude {}: {err}", fields[5]))?;
    let modification_date = NaiveDate::parse_from_str(fields[18], "%Y-%m-%d")
        .map_err(|err| format!("invalid modification date {}: {err}", fields[18]))?;

    let admin1_key = format!("{}.{}", fields[8], fields[10]);
    let admin2_key = format!("{}.{}.{}", fields[8], fields[10], fields[11]);

    Ok(Some(GeodataRecord {
        id,
        name: fields[1].to_string(),
        latitude,
        longitude,
        country_code: fields[8].to_string(),
        admin1_code: nullable_field(fields[10]),
        admin2_code: nullable_field(fields[11]),
        modification_date,
        admin1_name: admin1.get(&admin1_key).cloned(),
        admin2_name: admin2.get(&admin2_key).cloned(),
        alternate_names: nullable_field(fields[3]),
    }))
}

fn nullable_field(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

async fn insert_geodata_batch(pool: &PgPool, records: &[GeodataRecord]) -> Result<(), String> {
    if records.is_empty() {
        return Ok(());
    }

    let mut builder = QueryBuilder::new(
        r#"INSERT INTO geodata_places (
            id, name, longitude, latitude, "countryCode", "admin1Code", "admin2Code",
            "modificationDate", "admin1Name", "admin2Name", "alternateNames"
        ) "#,
    );

    builder.push_values(records, |mut row, record| {
        row.push_bind(record.id)
            .push_bind(&record.name)
            .push_bind(record.longitude)
            .push_bind(record.latitude)
            .push_bind(&record.country_code)
            .push_bind(&record.admin1_code)
            .push_bind(&record.admin2_code)
            .push_bind(record.modification_date)
            .push_bind(&record.admin1_name)
            .push_bind(&record.admin2_name)
            .push_bind(&record.alternate_names);
    });

    builder
        .build()
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

async fn create_geodata_indices(pool: &PgPool) -> Result<(), String> {
    sqlx::query("ALTER TABLE geodata_places ADD PRIMARY KEY (id) WITH (FILLFACTOR = 100)")
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;

    for (name, expression) in [
        (
            "idx_geodata_places_alternate_names",
            r#"gin (f_unaccent("alternateNames") gin_trgm_ops)"#,
        ),
        ("idx_geodata_places_name", "gin (f_unaccent(name) gin_trgm_ops)"),
        (
            "idx_geodata_places_admin1_name",
            r#"gin (f_unaccent("admin1Name") gin_trgm_ops)"#,
        ),
        (
            "idx_geodata_places_admin2_name",
            r#"gin (f_unaccent("admin2Name") gin_trgm_ops)"#,
        ),
    ] {
        let sql = format!("CREATE INDEX IF NOT EXISTS {name} ON geodata_places USING {expression}");
        sqlx::query(&sql)
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
    }

    Ok(())
}

struct NaturalEarthRecord {
    admin: String,
    admin_a3: String,
    record_type: String,
    coordinates: String,
}

async fn import_naturalearth_countries(pool: &PgPool, path: &Path) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|err| err.to_string())?;
    sqlx::query(
        r#"
        CREATE TABLE naturalearth_countries_tmp (
            LIKE naturalearth_countries INCLUDING ALL EXCLUDING INDEXES
        )
        "#,
    )
    .execute(&mut *tx)
    .await
    .map_err(|err| err.to_string())?;
    sqlx::query("DROP TABLE IF EXISTS naturalearth_countries")
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    sqlx::query("ALTER TABLE naturalearth_countries_tmp RENAME TO naturalearth_countries")
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    tx.commit().await.map_err(|err| err.to_string())?;

    println!("geodata import: starting Natural Earth countries import");
    let start = Instant::now();
    let path = path.to_path_buf();
    let (tx, mut rx) = mpsc::channel(MAX_IN_FLIGHT);
    let read_handle = spawn_blocking(move || -> Result<usize, String> {
        let file = std::fs::File::open(&path)
            .map_err(|err| format!("Geodata file {} not found: {err}", path.display()))?;
        let parsed: GeoJsonRoot = serde_json::from_reader(BufReader::new(file))
            .map_err(|err| format!("Invalid GeoJSON FeatureCollection: {err}"))?;
        if parsed.type_ != "FeatureCollection" {
            return Err("Invalid GeoJSON FeatureCollection".into());
        }

        let mut batch = Vec::with_capacity(BATCH_SIZE);
        let mut count = 0usize;
        for feature in parsed.features {
            for record in naturalearth_records_from_feature(feature)? {
                batch.push(record);
                count += 1;
                if batch.len() >= BATCH_SIZE {
                    if tx.blocking_send(batch).is_err() {
                        return Ok(count);
                    }
                    batch = Vec::with_capacity(BATCH_SIZE);
                }
            }
        }

        if !batch.is_empty() {
            let _ = tx.blocking_send(batch);
        }
        Ok(count)
    });

    let pool = pool.clone();
    let mut futures = FuturesUnordered::new();
    while let Some(batch) = rx.recv().await {
        let pool = pool.clone();
        futures.push(async move { insert_naturalearth_batch(&pool, &batch).await });

        if futures.len() >= MAX_IN_FLIGHT {
            if let Some(result) = futures.next().await {
                result?;
            }
        }
    }

    while let Some(result) = futures.next().await {
        result?;
    }

    let count = read_handle
        .await
        .map_err(|err| err.to_string())??;

    sqlx::query(
        "ALTER TABLE naturalearth_countries ADD PRIMARY KEY (id) WITH (FILLFACTOR = 100)",
    )
    .execute(&pool)
    .await
    .map_err(|err| err.to_string())?;

    let duration = start.elapsed().as_secs_f64();
    println!(
        "geodata import: successfully imported {count} Natural Earth records in {duration:.2}s"
    );
    Ok(())
}

fn naturalearth_records_from_feature(
    feature: GeoJsonFeature,
) -> Result<Vec<NaturalEarthRecord>, String> {
    let geometry_type = feature.geometry.type_;
    let admin = feature.properties.admin;
    let admin_a3 = feature.properties.admin_a3;
    let record_type = feature.properties.type_;
    let mut records = Vec::new();

    for entry in feature.geometry.coordinates {
        let ring = if geometry_type == "MultiPolygon" {
            entry
                .as_array()
                .and_then(|polygon| polygon.first())
                .and_then(|ring| ring.as_array())
                .ok_or_else(|| "Invalid MultiPolygon coordinates".to_string())?
        } else {
            entry
                .as_array()
                .ok_or_else(|| "Invalid Polygon coordinates".to_string())?
        };

        let mut points = Vec::new();
        for point in ring {
            let coords = point
                .as_array()
                .ok_or_else(|| "Invalid coordinate pair".to_string())?;
            let lon = coords
                .first()
                .and_then(|value| value.as_f64())
                .ok_or_else(|| "Invalid longitude".to_string())?;
            let lat = coords
                .get(1)
                .and_then(|value| value.as_f64())
                .ok_or_else(|| "Invalid latitude".to_string())?;
            points.push(format!("({lon},{lat})"));
        }

        records.push(NaturalEarthRecord {
            admin: admin.clone(),
            admin_a3: admin_a3.clone(),
            record_type: record_type.clone(),
            coordinates: format!("({})", points.join(", ")),
        });

        if geometry_type == "Polygon" {
            break;
        }
    }

    Ok(records)
}

async fn insert_naturalearth_batch(
    pool: &PgPool,
    records: &[NaturalEarthRecord],
) -> Result<(), String> {
    if records.is_empty() {
        return Ok(());
    }

    let mut builder = QueryBuilder::new(
        r#"INSERT INTO naturalearth_countries (admin, admin_a3, type, coordinates) "#,
    );
    builder.push_values(records, |mut row, record| {
        row.push_bind(&record.admin)
            .push_bind(&record.admin_a3)
            .push_bind(&record.record_type);
        row.push(format!("'{}'::polygon", record.coordinates.replace('\'', "''")));
    });

    builder
        .build()
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct GeoJsonRoot {
    #[serde(rename = "type")]
    type_: String,
    features: Vec<GeoJsonFeature>,
}

#[derive(Debug, Deserialize)]
struct GeoJsonFeature {
    properties: GeoJsonProperties,
    geometry: GeoJsonGeometry,
}

#[derive(Debug, Deserialize)]
struct GeoJsonProperties {
    #[serde(rename = "ADMIN")]
    admin: String,
    #[serde(rename = "ADM0_A3")]
    admin_a3: String,
    #[serde(rename = "TYPE")]
    type_: String,
}

#[derive(Debug, Deserialize)]
struct GeoJsonGeometry {
    #[serde(rename = "type")]
    type_: String,
    coordinates: Vec<serde_json::Value>,
}
