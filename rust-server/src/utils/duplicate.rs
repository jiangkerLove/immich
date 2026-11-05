use crate::models::response::asset::AssetResponse;

pub fn suggest_duplicate_keep_asset_ids(assets: &[AssetResponse]) -> Vec<String> {
    suggest_duplicate(assets)
        .map(|asset| vec![asset.id.to_string()])
        .unwrap_or_default()
}

fn suggest_duplicate(assets: &[AssetResponse]) -> Option<&AssetResponse> {
    if assets.is_empty() {
        return None;
    }

    let mut sorted: Vec<&AssetResponse> = assets.iter().collect();
    sorted.sort_by_key(|asset| file_size(asset));

    let largest = file_size(sorted.last()?);
    sorted.retain(|asset| file_size(asset) == largest);

    if sorted.len() >= 2 {
        sorted.sort_by_key(|asset| exif_count(asset));
    }

    sorted.last().copied()
}

fn file_size(asset: &AssetResponse) -> i64 {
    asset
        .exif_info
        .as_ref()
        .and_then(|value| value.get("fileSizeInByte"))
        .and_then(|value| value.as_i64())
        .unwrap_or(0)
}

fn exif_count(asset: &AssetResponse) -> usize {
    asset
        .exif_info
        .as_ref()
        .and_then(|value| value.as_object())
        .map(|fields| fields.values().filter(|value| json_truthy(value)).count())
        .unwrap_or(0)
}

/// Matches JavaScript `Boolean` / `filter(Boolean)` used by `getExifCount`.
fn json_truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(flag) => *flag,
        serde_json::Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        serde_json::Value::String(text) => !text.is_empty(),
        serde_json::Value::Array(items) => !items.is_empty(),
        serde_json::Value::Object(map) => !map.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::json_truthy;
    use serde_json::json;

    #[test]
    fn falsy_exif_values_match_javascript_boolean() {
        assert!(!json_truthy(&json!(null)));
        assert!(!json_truthy(&json!(false)));
        assert!(!json_truthy(&json!(0)));
        assert!(!json_truthy(&json!(0.0)));
        assert!(!json_truthy(&json!("")));
        assert!(json_truthy(&json!(true)));
        assert!(json_truthy(&json!(1)));
        assert!(json_truthy(&json!("x")));
    }
}
