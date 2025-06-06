use std::string::String;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub email: String,
    pub exp: i64,
}

pub async fn cors(req: Request, next: Next) -> Result<Response, StatusCode> {
    let mut response = if req.method() == Method::OPTIONS {
        Response::new(Body::from("ok"))
    } else {
        next.run(req).await
    };
    let headers = response.headers_mut();
    headers.insert("Access-Control-Allow-Credentials", HeaderValue::from_str("true").unwrap());
    headers.insert("Access-Control-Allow-Origin", HeaderValue::from_str("*").unwrap());
    headers.insert("Access-Control-Allow-Methods", HeaderValue::from_str("OPTIONS,GET,POST,DELETE,PUT").unwrap());
    headers.insert("Access-Control-Allow-Headers", HeaderValue::from_str("*").unwrap());
    headers.insert("Access-Control-Max-Age", HeaderValue::from_str("3600").unwrap());
    return Ok(response);
}