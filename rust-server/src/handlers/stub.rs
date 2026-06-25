use crate::models::response::response::ErrorResp;

pub async fn not_implemented() -> ErrorResp {
    ErrorResp::NotImplemented(
        "This endpoint is registered but not yet implemented in rust-server".to_string(),
    )
}
