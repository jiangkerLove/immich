use std::path::{Path, PathBuf};

use crate::models::dto::env::EnvDto;

pub const CITIES_FILE: &str = "cities500.txt";

#[derive(Debug, Clone)]
pub struct GeodataPaths {
    pub date_file: PathBuf,
    pub admin1: PathBuf,
    pub admin2: PathBuf,
    pub cities500: PathBuf,
    pub natural_earth_countries: PathBuf,
}

pub fn resolve_geodata_paths(env: &EnvDto) -> GeodataPaths {
    let geodata_dir = resolve_geodata_dir(env);
    GeodataPaths {
        date_file: geodata_dir.join("geodata-date.txt"),
        admin1: geodata_dir.join("admin1CodesASCII.txt"),
        admin2: geodata_dir.join("admin2Codes.txt"),
        cities500: geodata_dir.join(CITIES_FILE),
        natural_earth_countries: geodata_dir.join("ne_10m_admin_0_countries.geojson"),
    }
}

fn resolve_geodata_dir(env: &EnvDto) -> PathBuf {
    if let Some(build_data) = env.immich_build_data.as_ref() {
        let path = PathBuf::from(build_data).join("geodata");
        if path.is_dir() {
            return path;
        }
    }

    for candidate in candidate_geodata_dirs() {
        if candidate.is_dir() {
            return candidate;
        }
    }

    PathBuf::from("/build/geodata")
}

fn candidate_geodata_dirs() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(current) = std::env::current_dir() {
        paths.push(current.join("server/build/geodata"));
        if current.ends_with("rust-server") {
            paths.push(
                current
                    .parent()
                    .unwrap_or(&current)
                    .join("server/build/geodata"),
            );
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join("geodata"));
            paths.push(dir.join("../server/build/geodata"));
        }
    }

    paths.push(PathBuf::from("/build/geodata"));
    paths
}

pub fn geodata_dir_exists(paths: &GeodataPaths) -> bool {
    paths.date_file.is_file()
}

pub fn missing_geodata_file(paths: &GeodataPaths) -> Option<&Path> {
    for path in [
        &paths.date_file,
        &paths.admin1,
        &paths.admin2,
        &paths.cities500,
        &paths.natural_earth_countries,
    ] {
        if !path.is_file() {
            return Some(path.as_path());
        }
    }
    None
}
