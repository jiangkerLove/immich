use std::sync::Arc;
use axum::extract::{Request, State};
use axum::http;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

use crate::app_state::AppState;

pub async fn auth(State(app_state): State<Arc<AppState>>, mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    match path {
        "/user/register" => {
            Ok(next.run(req).await)
        }
        "/user/login" => {
            Ok(next.run(req).await)
        }
        _ => {
            let auth_header = req.headers()
                .get(http::header::AUTHORIZATION)
                .and_then(|header| header.to_str().ok());

            Ok(next.run(req).await)
        }
    }
}
