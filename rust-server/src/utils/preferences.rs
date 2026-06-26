use serde_json::Value;

pub fn default_preferences_json() -> Value {
    serde_json::json!({
        "albums": { "defaultAssetOrder": "desc" },
        "folders": { "enabled": false, "sidebarWeb": false },
        "memories": { "enabled": true, "duration": 5 },
        "people": { "enabled": true, "sidebarWeb": false, "minimumFaces": 3 },
        "sharedLinks": { "enabled": true, "sidebarWeb": false },
        "ratings": { "enabled": false },
        "tags": { "enabled": true, "sidebarWeb": false },
        "emailNotifications": {
            "enabled": true,
            "albumInvite": true,
            "albumUpdate": true
        },
        "download": {
            "archiveSize": 4_294_967_296_i64,
            "includeEmbeddedVideos": false
        },
        "purchase": {
            "showSupportBadge": true,
            "hideBuyButtonUntil": "2022-02-11T16:00:00.000Z"
        },
        "cast": { "gCastEnabled": false }
    })
}

pub fn merge_preferences(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            for (key, patch_value) in patch_map {
                match base_map.get_mut(&key) {
                    Some(existing) if existing.is_object() && patch_value.is_object() => {
                        merge_preferences(existing, patch_value);
                    }
                    _ => {
                        base_map.insert(key, patch_value);
                    }
                }
            }
        }
        (base_slot, patch) => *base_slot = patch,
    }
}

pub fn resolve_preferences(stored: Value) -> Value {
    let mut preferences = default_preferences_json();
    merge_preferences(&mut preferences, stored);
    preferences
}
