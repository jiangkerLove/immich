use std::time::Instant;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::utils::telemetry;

pub async fn track_http_metrics(request: Request, next: Next) -> Response {
    if !telemetry::api_metrics_enabled() {
        return next.run(request).await;
    }

    let start = Instant::now();
    let response = next.run(request).await;
    telemetry::record_http_request(start.elapsed().as_secs_f64() * 1000.0, response.status().as_u16());
    response
}
