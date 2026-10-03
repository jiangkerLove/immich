use serde_json::Value;

pub fn default_preferences_json() -> Value {
    serde_json::json!({
        "albums": { "defaultAssetOrder": "desc" },
        "folders": { "enabled": false, "sidebarWeb": false },
        "memories": { "enabled": true, "duration": 5, "sidebarWeb": false },
        "people": { "enabled": true, "sidebarWeb": false, "minimumFaces": 3 },
        "sharedLinks": { "enabled": true, "sidebarWeb": false },
        "ratings": { "enabled": false },
        "tags": { "enabled": false, "sidebarWeb": false },
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
        "cast": { "gCastEnabled": false },
        "recentlyAdded": { "sidebarWeb": false }
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
    let defaults = default_preferences_json();
    let mut preferences = defaults.clone();
    merge_preferences(&mut preferences, stored);
    fill_missing_defaults(&mut preferences, &defaults);
    preferences
}

/// A stored `null` must not erase a default object the web reads without a fallback.
fn fill_missing_defaults(target: &mut Value, defaults: &Value) {
    let (Value::Object(target_map), Value::Object(default_map)) = (target, defaults) else {
        return;
    };
    for (key, default_value) in default_map {
        match target_map.get_mut(key) {
            Some(existing)
                if existing.is_null() || (default_value.is_object() && !existing.is_object()) =>
            {
                *existing = default_value.clone();
            }
            Some(existing) if existing.is_object() && default_value.is_object() => {
                fill_missing_defaults(existing, default_value);
            }
            None => {
                target_map.insert(key.clone(), default_value.clone());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_preferences;
    use serde_json::json;

    #[test]
    fn defaults_cover_fields_the_web_reads_directly() {
        let preferences = resolve_preferences(json!({
            "recentlyAdded": null,
            "memories": { "enabled": true, "duration": 5 }
        }));
        assert_eq!(preferences["recentlyAdded"]["sidebarWeb"], false);
        assert_eq!(preferences["memories"]["enabled"], true);
        assert_eq!(preferences["memories"]["duration"], 5);
        assert_eq!(preferences["memories"]["sidebarWeb"], false);
        assert_eq!(preferences["people"]["enabled"], true);
        assert_eq!(preferences["people"]["sidebarWeb"], false);
        assert_eq!(preferences["sharedLinks"]["enabled"], true);
        assert_eq!(preferences["sharedLinks"]["sidebarWeb"], false);
        assert_eq!(preferences["tags"]["enabled"], false);
        assert_eq!(preferences["tags"]["sidebarWeb"], false);
        assert_eq!(preferences["folders"]["enabled"], false);
        assert_eq!(preferences["folders"]["sidebarWeb"], false);
        assert_eq!(preferences["ratings"]["enabled"], false);
        assert_eq!(preferences["cast"]["gCastEnabled"], false);
        assert_eq!(preferences["download"]["includeEmbeddedVideos"], false);
        assert!(preferences["download"]["archiveSize"].is_number());
        assert_eq!(preferences["purchase"]["showSupportBadge"], true);
        assert!(preferences["purchase"]["hideBuyButtonUntil"].is_string());
        assert_eq!(preferences["albums"]["defaultAssetOrder"], "desc");
    }
}
