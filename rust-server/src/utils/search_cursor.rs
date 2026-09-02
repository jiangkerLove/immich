use serde::{Deserialize, Serialize};

use crate::models::response::response::ErrorResp;

#[derive(Debug, Serialize, Deserialize)]
struct SearchCursorPayload {
    offset: i64,
}

pub fn encode_search_cursor(offset: i64) -> String {
    let payload = SearchCursorPayload { offset };
    base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        serde_json::to_string(&payload).unwrap_or_default(),
    )
}

pub fn decode_search_cursor(cursor: Option<&str>) -> Result<i64, ErrorResp> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };

    let bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, cursor)
        .map_err(|_| ErrorResp::BadRequest("Invalid cursor".to_string()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| ErrorResp::BadRequest("Invalid cursor".to_string()))?;
    let payload: SearchCursorPayload = serde_json::from_str(&text)
        .map_err(|_| ErrorResp::BadRequest("Invalid cursor".to_string()))?;
    if payload.offset < 0 {
        return Err(ErrorResp::BadRequest("Invalid cursor".to_string()));
    }
    Ok(payload.offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_cursor() {
        let encoded = encode_search_cursor(250);
        assert_eq!(decode_search_cursor(Some(&encoded)).unwrap(), 250);
    }

    #[test]
    fn missing_cursor_is_zero() {
        assert_eq!(decode_search_cursor(None).unwrap(), 0);
    }

    #[test]
    fn invalid_cursor_rejected() {
        assert!(decode_search_cursor(Some("not-a-cursor")).is_err());
    }
}
