use rust_server::service::admin;
use rust_server::service::bootstrap::{self, ServerMode};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("immich-admin") {
        admin::run(&args[2..]).await;
        return;
    }

    let argv_mode = args.get(1).map(|s| s.as_str());
    if argv_mode == Some("maintenance") {
        bootstrap::run(ServerMode::Maintenance).await;
        return;
    }

    let settings = bootstrap::load_env();
    let mode = bootstrap::resolve_server_mode(&settings, argv_mode).await;
    bootstrap::run(mode).await;
}
