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
        .map(|fields| {
            fields
                .values()
                .filter(|value| {
                    !value.is_null()
                        && value.as_str().map(|text| !text.is_empty()).unwrap_or(true)
                })
                .count()
        })
        .unwrap_or(0)
}
