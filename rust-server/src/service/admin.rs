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
            let password = args
                .iter()
                .skip(1)
                .find(|arg| !arg.starts_with('-'))
                .map(String::as_str);
            let keep_sessions = args.iter().any(|arg| arg == "--keep-sessions");
            if let Err(err) = reset_admin_password(&settings, password, !keep_sessions).await {
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
        "migration-status" => {
            if let Err(err) = migration_status(&settings).await {
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
  reset-admin-password [pw] [--keep-sessions]
                            Reset admin password (generates one if omitted).
                            Invalidates sessions by default (TS prompt default).
                            Pass --keep-sessions to retain existing sessions.
  grant-admin <email>       Grant admin privileges
  revoke-admin <email>      Revoke admin privileges
  schema-check              Verify schema tables vs sqlx 1_baseline.sql
  run-migrations            Auto-check + apply sqlx migrations (baseline + pending)
  migration-status          Print sqlx / baseline_lock / kysely drift status
  media-location            Print current media location
  change-media-location <old> <new> [--yes]
                            Rewrite stored file paths after moving media
  enable-maintenance-mode   Enable maintenance mode (Redis AppRestart + JWT login URL)
  disable-maintenance-mode  Disable maintenance mode (Redis AppRestart)
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

async fn reset_admin_password(
    settings: &EnvDto,
    password: Option<&str>,
    invalidate_sessions: bool,
) -> Result<(), String> {
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
    let hash =
        bcrypt::hash(&password, crate::constants::SALT_ROUNDS).map_err(|err| err.to_string())?;

    sqlx::query(r#"UPDATE "user" SET password = $1 WHERE id = $2"#)
        .bind(hash)
        .bind(admin_id)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;

    if invalidate_sessions {
        crate::models::db::sessions::SessionPO::invalidate_all_except(&pool, &admin_id, None)
            .await
            .map_err(|err| err.to_string())?;
    }

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
    let pool = connect_pool(settings).await?;
    crate::service::database_migrations::run(&pool, settings)
        .await
        .map_err(|err| err.to_string())
}

async fn migration_status(settings: &EnvDto) -> Result<(), String> {
    let pool = connect_pool(settings).await?;
    let status = crate::service::database_migrations::status(&pool)
        .await
        .map_err(|err| err.to_string())?;
    crate::service::database_migrations::print_status(&status);
    if !status.sqlx_pending.is_empty() {
        println!("pending sqlx:");
        for (version, description) in &status.sqlx_pending {
            println!("  - {version} {description}");
        }
    }
    if !status.kysely_ahead_of_lock.is_empty() {
        println!("kysely ahead of baseline_lock (need sqlx absorb):");
        for name in &status.kysely_ahead_of_lock {
            println!("  - {name}");
        }
    }
    Ok(())
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

    let updated =
        crate::models::db::media_location::migrate_file_paths(&pool, old_value, new_value)
            .await
            .map_err(|err| err.to_string())?;

    if updated == 0 {
        println!("No rows were updated");
    } else {
        println!("Updated {updated} row(s). Set IMMICH_MEDIA_LOCATION={new_value} and restart.");
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
    use crate::models::dto::maintenance::{
        MaintenanceAction, MaintenanceModeState, SetMaintenanceModeReq,
    };
    use crate::service::maintenance::{generate_maintenance_secret, sign_maintenance_jwt};
    use crate::service::server_events;

    let pool = connect_pool(settings).await?;
    let existing = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT value FROM system_metadata WHERE key = 'maintenance-mode'"#,
    )
    .fetch_optional(&pool)
    .await
    .map_err(|err| err.to_string())?;

    let already = existing
        .as_ref()
        .and_then(|value| value.get("isMaintenanceMode"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let (secret, already_enabled) = if already {
        let secret = existing
            .as_ref()
            .and_then(|value| value.get("secret"))
            .and_then(|value| value.as_str())
            .ok_or_else(|| "Maintenance mode is on but secret is missing".to_string())?
            .to_string();
        (secret, true)
    } else {
        let secret = generate_maintenance_secret();
        let state = MaintenanceModeState {
            is_maintenance_mode: true,
            secret: Some(secret.clone()),
            action: Some(SetMaintenanceModeReq {
                action: MaintenanceAction::Start,
                restore_backup_filename: None,
            }),
        };
        sqlx::query(
            r#"
            INSERT INTO system_metadata (key, value)
            VALUES ('maintenance-mode', $1)
            ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value
            "#,
        )
        .bind(serde_json::to_value(&state).map_err(|err| err.to_string())?)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;

        let redis_url = server_events::redis_url_from_env(settings);
        server_events::publish_app_restart(&redis_url, true).await?;
        (secret, false)
    };

    let jwt = sign_maintenance_jwt(&secret, "cli-admin").map_err(|err| err.to_string())?;
    let host = settings.immich_host.as_deref().unwrap_or("localhost");
    let port = settings.immich_port.unwrap_or(2283);
    let auth_url = format!("http://{host}:{port}/maintenance?token={jwt}");

    if already_enabled {
        println!("The server is already in maintenance mode!");
    } else {
        println!("Maintenance mode has been enabled.");
        println!(
            "(signaled running rust-server via Redis AppRestart — process manager should restart)"
        );
    }
    println!("\nLog in using the following URL:");
    println!("{auth_url}");
    Ok(())
}

async fn disable_maintenance_mode(settings: &EnvDto) -> Result<(), String> {
    use crate::models::dto::maintenance::MaintenanceModeState;
    use crate::service::server_events;

    let pool = connect_pool(settings).await?;
    let existing = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT value FROM system_metadata WHERE key = 'maintenance-mode'"#,
    )
    .fetch_optional(&pool)
    .await
    .map_err(|err| err.to_string())?;

    let already_disabled = existing
        .as_ref()
        .and_then(|value| value.get("isMaintenanceMode"))
        .and_then(|value| value.as_bool())
        .map(|on| !on)
        .unwrap_or(true);

    if already_disabled {
        println!("The server is already out of maintenance mode!");
        return Ok(());
    }

    let state = MaintenanceModeState {
        is_maintenance_mode: false,
        secret: None,
        action: None,
    };
    sqlx::query(
        r#"
        INSERT INTO system_metadata (key, value)
        VALUES ('maintenance-mode', $1)
        ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value
        "#,
    )
    .bind(serde_json::to_value(&state).map_err(|err| err.to_string())?)
    .execute(&pool)
    .await
    .map_err(|err| err.to_string())?;

    let redis_url = server_events::redis_url_from_env(settings);
    server_events::publish_app_restart(&redis_url, false).await?;

    println!("Maintenance mode has been disabled.");
    println!(
        "(signaled running rust-server via Redis AppRestart — process manager should restart)"
    );
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
