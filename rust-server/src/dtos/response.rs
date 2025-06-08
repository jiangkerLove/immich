use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ErrorDto {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Request parameter error: {0}")]
    ReqParamError(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Server error: {0}")]
    ServerError(String),
}

impl IntoResponse for ErrorDto {
    fn into_response(self) -> Response {
        let (code, msg, error) = match self {
            ErrorDto::Unauthorized(err) => (StatusCode::UNAUTHORIZED, err, "Unauthorized"),
            ErrorDto::ReqParamError(err) => (StatusCode::BAD_REQUEST, err, ""),
            ErrorDto::DatabaseError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string(), ""),
            ErrorDto::ServerError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err, ""),
        };
        (
            code,
            json!({
                "message": msg,
                "error": error,
                "statusCode": code.as_u16(),
                "correlationId": "tp700cb8"
            }).to_string()
        ).into_response()
    }
}
