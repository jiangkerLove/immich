use serde_json::{json, Value};

#[derive(Debug, Clone, Copy)]
pub enum ConfigVisibility {
    User,
    Public,
}

pub fn filter_config(config: &Value, visibility: ConfigVisibility) -> Value {
    match visibility {
        ConfigVisibility::Public => filter_public_config(config),
        ConfigVisibility::User => filter_user_config(config),
    }
}

fn filter_public_config(config: &Value) -> Value {
    json!({
        "oauth": pick_nested(config, &["oauth"], &[
            "autoLaunch",
            "buttonText",
            "enabled",
            "accountManagementUrl",
        ]),
        "passwordLogin": pick_nested(config, &["passwordLogin"], &["enabled"]),
        "theme": pick_nested(config, &["theme"], &["customCss"]),
        "server": pick_nested(config, &["server"], &["loginPageMessage"]),
    })
}

fn filter_user_config(config: &Value) -> Value {
    json!({
        "oauth": pick_nested(config, &["oauth"], &[
            "autoLaunch",
            "buttonText",
            "enabled",
            "accountManagementUrl",
        ]),
        "passwordLogin": pick_nested(config, &["passwordLogin"], &["enabled"]),
        "theme": pick_nested(config, &["theme"], &["customCss"]),
        "machineLearning": json!({
            "enabled": get_bool(config, &["machineLearning", "enabled"]),
            "clip": pick_nested(config, &["machineLearning", "clip"], &["enabled"]),
            "duplicateDetection": pick_nested(
                config,
                &["machineLearning", "duplicateDetection"],
                &["enabled"],
            ),
            "facialRecognition": pick_nested(
                config,
                &["machineLearning", "facialRecognition"],
                &["enabled", "minFaces"],
            ),
            "ocr": pick_nested(config, &["machineLearning", "ocr"], &["enabled"]),
        }),
        "map": pick_nested(config, &["map"], &["enabled", "lightStyle", "darkStyle"]),
        "reverseGeocoding": pick_nested(config, &["reverseGeocoding"], &["enabled"]),
        "ffmpeg": json!({
            "realtime": pick_nested(
                config,
                &["ffmpeg", "realtime"],
                &["enabled", "videoCodecs", "resolutions"],
            ),
        }),
        "image": json!({
            "thumbnail": pick_nested(config, &["image", "thumbnail"], &["size"]),
            "preview": pick_nested(config, &["image", "preview"], &["size"]),
            "fullsize": pick_nested(config, &["image", "fullsize"], &["enabled"]),
        }),
        "trash": pick_nested(config, &["trash"], &["enabled", "days"]),
        "server": pick_nested(
            config,
            &["server"],
            &["externalDomain", "publicUsers", "loginPageMessage"],
        ),
        "user": pick_nested(config, &["user"], &["deleteDelay"]),
    })
}

fn pick_nested(config: &Value, path: &[&str], fields: &[&str]) -> Value {
    let Some(obj) = get_object(config, path) else {
        return Value::Object(Default::default());
    };

    let mut map = serde_json::Map::new();
    for field in fields {
        if let Some(value) = obj.get(*field) {
            map.insert(field.to_string(), value.clone());
        }
    }
    Value::Object(map)
}

fn get_object<'a>(config: &'a Value, path: &[&str]) -> Option<&'a serde_json::Map<String, Value>> {
    path.iter()
        .try_fold(config, |current, key| current.get(*key))
        .and_then(|value| value.as_object())
}

fn get_bool(config: &Value, path: &[&str]) -> bool {
    path.iter()
        .try_fold(config, |current, key| current.get(*key))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::system_config::defaults;

    #[test]
    fn public_config_hides_user_fields() {
        let config = defaults();
        let public = filter_config(&config, ConfigVisibility::Public);

        assert!(public.get("image").is_none());
        assert!(public.get("trash").is_none());
        assert!(public.get("job").is_none());
        assert_eq!(
            public
                .get("server")
                .and_then(|value| value.get("loginPageMessage"))
                .and_then(|value| value.as_str()),
            config
                .get("server")
                .and_then(|value| value.get("loginPageMessage"))
                .and_then(|value| value.as_str())
        );
    }

    #[test]
    fn user_config_hides_admin_fields() {
        let config = defaults();
        let user = filter_config(&config, ConfigVisibility::User);

        assert!(user.get("job").is_none());
        assert!(user
            .get("oauth")
            .and_then(|value| value.get("clientSecret"))
            .is_none());
        assert_eq!(
            user.get("image")
                .and_then(|image| image.get("thumbnail"))
                .and_then(|thumb| thumb.get("size"))
                .and_then(|size| size.as_i64()),
            config
                .get("image")
                .and_then(|image| image.get("thumbnail"))
                .and_then(|thumb| thumb.get("size"))
                .and_then(|size| size.as_i64())
        );
    }
}
