use axum::Router;

use crate::app_state::AppState;
use crate::routes::{maintenance, server, static_web};

pub fn router(web_root: Option<&std::path::Path>) -> Router<AppState> {
    let api = Router::new()
        .merge(server::public_router())
        .merge(maintenance::public_router());

    if let Some(web_root) = web_root {
        api.merge(static_web::fallback_router(web_root))
    } else {
        api
    }
}
