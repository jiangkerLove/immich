use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ErrorResp {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Request parameter error: {0}")]
    ReqParamError(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

pub fn handler_err(error_dto: ErrorResp) -> Response {
    error_dto.into_response()
}

impl IntoResponse for ErrorResp {
    fn into_response(self) -> Response {
        let (code, msg, error) = match self {
            ErrorResp::Unauthorized(err) => (StatusCode::UNAUTHORIZED, err, "Unauthorized"),
            ErrorResp::Forbidden(err) => (StatusCode::FORBIDDEN, err, "Forbidden"),
            ErrorResp::BadRequest(err) => (StatusCode::BAD_REQUEST, err, "Bad Request"),
            ErrorResp::ReqParamError(err) => (StatusCode::BAD_REQUEST, err, ""),
            ErrorResp::DatabaseError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string(), ""),
            ErrorResp::ServerError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err, ""),
            ErrorResp::NotFound(err) => (StatusCode::NOT_FOUND, err, "Not Found"),
            ErrorResp::NotImplemented(err) => (StatusCode::NOT_IMPLEMENTED, err, "Not Implemented"),
        };
        (
            code,
            json!({
                "message": msg,
                "error": error,
                "statusCode": code.as_u16(),
                "correlationId": "tp700cb8"
            })
            .to_string(),
        )
            .into_response()
    }
}
