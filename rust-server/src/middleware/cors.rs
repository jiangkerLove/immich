use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

pub async fn cors(req: Request, next: Next) -> Result<Response, StatusCode> {
    let response = if req.method() == Method::OPTIONS {
        Response::new(Body::from("ok"))
    } else {
        next.run(req).await
    };

    let (mut parts, body) = response.into_parts();
    parts.headers.insert(
        "Access-Control-Allow-Credentials",
        HeaderValue::from_static("true"),
    );
    parts.headers.insert(
        "Access-Control-Allow-Origin",
        HeaderValue::from_static("*"),
    );
    parts.headers.insert(
        "Access-Control-Allow-Methods",
        HeaderValue::from_static("OPTIONS,GET,POST,DELETE,PUT,PATCH"),
    );
    parts.headers.insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("*"),
    );
    parts.headers.insert(
        "Access-Control-Max-Age",
        HeaderValue::from_static("3600"),
    );

    Ok(Response::from_parts(parts, body))
}
