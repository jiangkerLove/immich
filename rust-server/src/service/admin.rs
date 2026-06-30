use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use crate::constants::SERVER_VERSION;
use crate::models::dto::env::EnvDto;
use crate::service::bootstrap;
use crate::utils::storage::StoragePaths;

pub async fn run(args: &[String]) {
    let command = args.first().map(String::as_str).unwrap_or("help");
    let settings = bootstrap::load_env();

    match command {
        "version" => {
            println!("v{SERVER_VERSION}");
        }
        "list-users" => {
            if let Err(err) = list_users(&settings).await {
                eprintln!("{err}");
            }
        }
        "reset-admin-password" => {
            let password = args.get(1).map(String::as_str);
            if let Err(err) = reset_admin_password(&settings, password).await {
                eprintln!("{err}");
            }
        }
        "grant-admin" => {
            let email = args.get(1).ok_or_else(|| "email required".to_string());
            match email {
                Ok(email) => {
                    if let Err(err) = set_admin(&settings, email, true).await {
                        eprintln!("{err}");
                    } else {
                        println!("Admin access has been granted to {email}");
                    }
                }
                Err(err) => eprintln!("{err}"),
            }
        }
        "revoke-admin" => {
            let email = args.get(1).ok_or_else(|| "email required".to_string());
            match email {
                Ok(email) => {
                    if let Err(err) = set_admin(&settings, email, false).await {
                        eprintln!("{err}");
                    } else {
                        println!("Admin access has been revoked from {email}");
                    }
                }
                Err(err) => eprintln!("{err}"),
            }
        }
        "schema-check" => {
            if let Err(err) = schema_check(&settings).await {
                eprintln!("{err}");
            }
        }
        "run-migrations" => {
            if let Err(err) = run_migrations(&settings).await {
                eprintln!("{err}");
            }
        }
        "media-location" => {
            let media = StoragePaths::new(resolve_media_location(&settings));
            println!("{}", media.media_location().display());
        }
        "change-media-location" => {
            let old_value = args.get(1).map(String::as_str);
            let new_value = args.get(2).map(String::as_str);
            let assume_yes = args.iter().any(|arg| arg == "--yes" || arg == "-y");
            if let Err(err) =
                change_media_location(&settings, old_value, new_value, assume_yes).await
            {
                eprintln!("{err}");
            }
        }
        "enable-maintenance-mode" => {
            if let Err(err) = enable_maintenance_mode(&settings).await {
                eprintln!("{err}");
            }
        }
        "disable-maintenance-mode" => {
            if let Err(err) = disable_maintenance_mode(&settings).await {
                eprintln!("{err}");
            }
        }
        "enable-password-login" => {
            if let Err(err) = set_password_login(&settings, true).await {
                eprintln!("{err}");
            } else {
                println!("Password login has been enabled.");
            }
        }
        "disable-password-login" => {
            if let Err(err) = set_password_login(&settings, false).await {
                eprintln!("{err}");
            } else {
                println!("Password login has been disabled.");
            }
        }
        "enable-oauth-login" => {
            if let Err(err) = set_oauth_login(&settings, true).await {
                eprintln!("{err}");
            } else {
                println!("OAuth login has been enabled.");
            }
        }
        "disable-oauth-login" => {
            if let Err(err) = set_oauth_login(&settings, false).await {
                eprintln!("{err}");
            } else {
                println!("OAuth login has been disabled.");
            }
        }
        "help" | "--help" | "-h" => print_help(),
        other => {
            eprintln!("Unknown command: {other}");
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        r#"Immich admin CLI

Usage: rust-server immich-admin <command> [args]

Commands:
  version                   Print server version
  list-users                List users
  reset-admin-password [pw] Reset admin password (generates one if omitted)
  grant-admin <email>       Grant admin privileges
  revoke-admin <email>      Revoke admin privileges
  schema-check              Verify kysely migrations and init.sql schema drift
  run-migrations            Run pending Kysely database migrations
  media-location            Print current media location
  change-media-location <old> <new> [--yes]
                            Rewrite stored file paths after moving media
  enable-maintenance-mode   Enable maintenance mode
  disable-maintenance-mode  Disable maintenance mode
  enable-password-login     Enable password login
  disable-password-login    Disable password login
  enable-oauth-login        Enable OAuth login
  disable-oauth-login       Disable OAuth login
"#
    );
}

async fn connect_pool(settings: &EnvDto) -> Result<sqlx::PgPool, String> {
    let url = format!(
        "postgres://{}:{}@{}:{}/{}",
        settings.db_username,
        settings.db_password,
        settings.db_url,
        settings.db_port,
        settings.db_database_name,
    );
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .map_err(|err| err.to_string())
}

async fn list_users(settings: &EnvDto) -> Result<(), String> {
    let pool = connect_pool(settings).await?;
    let rows: Vec<(Uuid, String, String, bool)> = sqlx::query_as(
        r#"
        SELECT id, email, name, "isAdmin" as is_admin
        FROM "user"
        ORDER BY email
        "#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|err| err.to_string())?;

    for (id, email, name, is_admin) in rows {
        println!("{id}\t{email}\t{name}\tadmin={is_admin}");
    }
    Ok(())
}

async fn reset_admin_password(settings: &EnvDto, password: Option<&str>) -> Result<(), String> {
    let pool = connect_pool(settings).await?;
    let admin: Option<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, email
        FROM "user"
        WHERE "isAdmin" = TRUE AND status = 'active'
        ORDER BY "createdAt" ASC
        LIMIT 1
        "#,
    )
    .fetch_optional(&pool)
    .await
    .map_err(|err| err.to_string())?;

    let Some((admin_id, email)) = admin else {
        return Err("No active admin user found".to_string());
    };

    let password = password.map(str::to_string);
    let generated = password.is_none();
    let password = password.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let hash = bcrypt::hash(&password, crate::constants::SALT_ROUNDS)
        .map_err(|err| err.to_string())?;

    sqlx::query(r#"UPDATE "user" SET password = $1 WHERE id = $2"#)
        .bind(hash)
        .bind(admin_id)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;

    if generated {
        println!("The admin password has been updated to:\n{password}");
    } else {
        println!("The admin password has been updated for {email}.");
    }
    Ok(())
}

async fn set_admin(settings: &EnvDto, email: &str, is_admin: bool) -> Result<(), String> {
    let pool = connect_pool(settings).await?;
    let result = sqlx::query(r#"UPDATE "user" SET "isAdmin" = $1 WHERE email = $2"#)
        .bind(is_admin)
        .bind(email)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;

    if result.rows_affected() == 0 {
        return Err(format!("User not found: {email}"));
    }
    Ok(())
}

async fn schema_check(settings: &EnvDto) -> Result<(), String> {
    let pool = connect_pool(settings).await?;
    let report = crate::models::db::schema_check::run(&pool)
        .await
        .map_err(|err| err.to_string())?;

    if crate::models::db::schema_check::print_report(&report) {
        Ok(())
    } else {
        Err("Schema check failed".to_string())
    }
}

async fn run_migrations(settings: &EnvDto) -> Result<(), String> {
    crate::service::database_migrations::run(settings)
        .await
        .map_err(|err| err.to_string())
}

async fn set_password_login(settings: &EnvDto, enabled: bool) -> Result<(), String> {
    let pool = connect_pool(settings).await?;
    crate::utils::system_config::set_config_field(
        &pool,
        &["passwordLogin", "enabled"],
        serde_json::json!(enabled),
    )
    .await
    .map_err(|err| err.to_string())
}

async fn set_oauth_login(settings: &EnvDto, enabled: bool) -> Result<(), String> {
    let pool = connect_pool(settings).await?;
    crate::utils::system_config::set_config_field(
        &pool,
        &["oauth", "enabled"],
        serde_json::json!(enabled),
    )
    .await
    .map_err(|err| err.to_string())
}

async fn change_media_location(
    settings: &EnvDto,
    old_value: Option<&str>,
    new_value: Option<&str>,
    assume_yes: bool,
) -> Result<(), String> {
    let pool = connect_pool(settings).await?;
    let samples = crate::models::db::media_location::sample_file_paths(&pool)
        .await
        .map_err(|err| err.to_string())?;
    if !samples.is_empty() {
        println!("\nExamples from the database:");
        for path in &samples {
            println!("  - {path}");
        }
        println!();
    }

    let default_old = settings
        .immich_media_location
        .as_deref()
        .or(settings.upload_location.as_deref())
        .unwrap_or("./library");
    let old_value = old_value.unwrap_or(default_old);
    let new_value = new_value.ok_or_else(|| {
        "usage: change-media-location <old-path> <new-absolute-path> [--yes]".to_string()
    })?;

    if !std::path::Path::new(new_value).is_absolute() {
        return Err("Target media location must be an absolute path".to_string());
    }

    let mut source = old_value.trim_end_matches('/').to_string();
    if source.starts_with("./") {
        source = source[2..].to_string();
    }
    let target = new_value.trim_end_matches('/');

    println!("Previous value: {old_value}");
    println!("Current value:  {new_value}");
    println!("\nChanging from \"{source}/*\" to \"{target}/*\"\n");

    if !assume_yes {
        println!("Re-run with --yes to apply this change.");
        return Ok(());
    }

    let updated = crate::models::db::media_location::migrate_file_paths(&pool, old_value, new_value)
        .await
        .map_err(|err| err.to_string())?;

    if updated == 0 {
        println!("No rows were updated");
    } else {
        println!(
            "Updated {updated} row(s). Set IMMICH_MEDIA_LOCATION={new_value} and restart."
        );
    }

    let samples = crate::models::db::media_location::sample_file_paths(&pool)
        .await
        .map_err(|err| err.to_string())?;
    if !samples.is_empty() {
        println!("\nExamples after update:");
        for path in samples {
            println!("  - {path}");
        }
    }

    Ok(())
}

async fn enable_maintenance_mode(settings: &EnvDto) -> Result<(), String> {
    let pool = connect_pool(settings).await?;
    let secret = uuid::Uuid::new_v4().to_string();
    let payload = serde_json::json!({
        "isMaintenanceMode": true,
        "secret": secret,
        "action": { "action": "start" }
    });

    sqlx::query(
        r#"
        INSERT INTO system_metadata (key, value)
        VALUES ('maintenance-mode', $1)
        ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value
        "#,
    )
    .bind(payload)
    .execute(&pool)
    .await
    .map_err(|err| err.to_string())?;

    let host = settings.immich_host.as_deref().unwrap_or("localhost");
    let port = settings.immich_port.unwrap_or(2283);
    println!("Maintenance mode has been enabled.");
    println!("\nLog in using the following URL:");
    println!("http://{host}:{port}/admin/maintenance");
    Ok(())
}

async fn disable_maintenance_mode(settings: &EnvDto) -> Result<(), String> {
    let pool = connect_pool(settings).await?;
    sqlx::query(r#"DELETE FROM system_metadata WHERE key = 'maintenance-mode'"#)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;
    println!("Maintenance mode has been disabled.");
    let _ = settings;
    Ok(())
}

fn resolve_media_location(settings: &EnvDto) -> std::path::PathBuf {
    settings
        .immich_media_location
        .as_ref()
        .or(settings.upload_location.as_ref())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("./library"))
}
