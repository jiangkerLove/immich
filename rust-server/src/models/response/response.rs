use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ConfigIssue {
    pub code: String,
    pub path: Vec<serde_json::Value>,
    pub message: String,
    pub expected: Option<String>,
    pub input: Option<serde_json::Value>,
    pub values: Option<Vec<String>>,
    pub origin: Option<String>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub inclusive: Option<bool>,
}

impl ConfigIssue {
    pub fn to_json(&self) -> serde_json::Value {
        let mut body = serde_json::Map::new();
        body.insert("code".to_string(), json!(self.code));
        body.insert("path".to_string(), json!(self.path));
        body.insert("message".to_string(), json!(self.message));
        if let Some(expected) = &self.expected {
            body.insert("expected".to_string(), json!(expected));
        }
        if let Some(input) = &self.input {
            body.insert("input".to_string(), input.clone());
        }
        if let Some(values) = &self.values {
            body.insert("values".to_string(), json!(values));
        }
        if let Some(origin) = &self.origin {
            body.insert("origin".to_string(), json!(origin));
        }
        if let Some(minimum) = self.minimum {
            body.insert("minimum".to_string(), json!(minimum));
        }
        if let Some(maximum) = self.maximum {
            body.insert("maximum".to_string(), json!(maximum));
        }
        if let Some(inclusive) = self.inclusive {
            body.insert("inclusive".to_string(), json!(inclusive));
        }
        serde_json::Value::Object(body)
    }
}

pub fn format_config_issues(issues: &[ConfigIssue]) -> String {
    issues
        .iter()
        .map(|issue| format!("{} {}", config_issue_path(issue), issue.message))
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn format_system_config_file_error(issues: &[ConfigIssue]) -> String {
    let mut lines = vec!["Invalid system config: ".to_string()];
    for issue in issues {
        lines.push(format!(
            "  - [{}] {}",
            config_issue_path(issue),
            issue.message
        ));
    }
    lines.join("\n")
}

fn config_issue_path(issue: &ConfigIssue) -> String {
    issue
        .path
        .iter()
        .map(|segment| match segment {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Number(number) => number.to_string(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join(".")
}

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

    #[error("{}", format_config_issues(.0))]
    Validation(Vec<ConfigIssue>),
}

pub fn handler_err(error_dto: ErrorResp) -> Response {
    error_dto.into_response()
}

impl IntoResponse for ErrorResp {
    fn into_response(self) -> Response {
        // Match GlobalExceptionFilter: body is `{ message }` only (plus `errors`
        // for Zod). Nest's `error` / `statusCode` are stripped; correlation is a
        // header, not a body field.
        let (code, body) = match self {
            ErrorResp::Validation(issues) => (
                StatusCode::BAD_REQUEST,
                json!({
                    "message": "Validation failed",
                    "errors": issues.iter().map(ConfigIssue::to_json).collect::<Vec<_>>(),
                }),
            ),
            other => {
                let (code, msg) = match other {
                    ErrorResp::Unauthorized(err) => (StatusCode::UNAUTHORIZED, err),
                    ErrorResp::Forbidden(err) => (StatusCode::FORBIDDEN, err),
                    ErrorResp::BadRequest(err) => (StatusCode::BAD_REQUEST, err),
                    ErrorResp::ReqParamError(err) => (StatusCode::BAD_REQUEST, err),
                    ErrorResp::DatabaseError(err) => {
                        tracing::error!(error = %err, "database error");
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Internal server error".to_string(),
                        )
                    }
                    ErrorResp::ServerError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err),
                    ErrorResp::NotFound(err) => (StatusCode::NOT_FOUND, err),
                    ErrorResp::NotImplemented(err) => (StatusCode::NOT_IMPLEMENTED, err),
                    ErrorResp::Validation(_) => unreachable!(),
                };
                (code, json!({ "message": msg }))
            }
        };
        (code, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::ErrorResp;
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[tokio::test]
    async fn http_errors_are_message_only() {
        let response = ErrorResp::BadRequest("nope".into()).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body, serde_json::json!({ "message": "nope" }));
    }

    #[tokio::test]
    async fn database_errors_do_not_leak_sql() {
        let response =
            ErrorResp::DatabaseError(sqlx::Error::Protocol("SELECT boom".into())).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            body,
            serde_json::json!({ "message": "Internal server error" })
        );
    }
}
