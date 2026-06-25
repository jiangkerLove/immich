use std::path::Path;

use uuid::Uuid;

use crate::models::db::smart_search;
use crate::models::db::system_metadata::MachineLearningConfig;
use crate::models::response::response::ErrorResp;

enum ClipPayload {
    Text(String),
    Image(Vec<u8>, String),
}

pub async fn encode_clip_text(
    config: &MachineLearningConfig,
    text: &str,
    language: Option<&str>,
) -> Result<String, ErrorResp> {
    let mut textual = serde_json::json!({ "modelName": config.clip.model_name });
    if let Some(language) = language.filter(|l| !l.is_empty()) {
        textual["options"] = serde_json::json!({ "language": language });
    }
    let entries = serde_json::json!({
        "clip": {
            "textual": textual
        }
    });

    predict_clip(
        config,
        &entries.to_string(),
        ClipPayload::Text(text.to_string()),
    )
    .await
}

pub async fn encode_clip_image(
    config: &MachineLearningConfig,
    image_path: &Path,
) -> Result<String, ErrorResp> {
    let bytes = tokio::fs::read(image_path)
        .await
        .map_err(|e| ErrorResp::ServerError(format!("Failed to read image for CLIP: {e}")))?;

    let entries = serde_json::json!({
        "clip": {
            "visual": {
                "modelName": config.clip.model_name
            }
        }
    });

    let file_name = image_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image.jpg")
        .to_string();

    predict_clip(
        config,
        &entries.to_string(),
        ClipPayload::Image(bytes, file_name),
    )
    .await
}

pub async fn index_asset_image(
    pool: &sqlx::PgPool,
    config: &MachineLearningConfig,
    asset_id: &Uuid,
    image_path: &Path,
) -> Result<(), ErrorResp> {
    let embedding = encode_clip_image(config, image_path).await?;
    smart_search::upsert_embedding(pool, asset_id, &embedding)
        .await
        .map_err(ErrorResp::from)?;
    Ok(())
}

async fn predict_clip(
    config: &MachineLearningConfig,
    entries: &str,
    payload: ClipPayload,
) -> Result<String, ErrorResp> {
    let client = reqwest::Client::new();
    for url in &config.urls {
        let endpoint = format!("{}/predict", url.trim_end_matches('/'));
        let mut form = reqwest::multipart::Form::new().text("entries", entries.to_string());
        form = match &payload {
            ClipPayload::Text(text) => form.text("text", text.clone()),
            ClipPayload::Image(bytes, file_name) => {
                let part = reqwest::multipart::Part::bytes(bytes.clone())
                    .file_name(file_name.clone())
                    .mime_str("application/octet-stream")
                    .map_err(|e| ErrorResp::ServerError(e.to_string()))?;
                form.part("image", part)
            }
        };

        match client.post(&endpoint).multipart(form).send().await {
            Ok(response) if response.status().is_success() => {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    if let Some(embedding) = extract_clip_embedding(&body) {
                        return Ok(smart_search::normalize_embedding(&embedding));
                    }
                }
            }
            _ => continue,
        }
    }

    Err(ErrorResp::ServerError(
        "Machine learning CLIP encoding failed".to_string(),
    ))
}

fn extract_clip_embedding(body: &serde_json::Value) -> Option<String> {
    let clip = body.get("clip")?;
    if let Some(text) = clip.as_str() {
        return Some(text.to_string());
    }
    if clip.is_array() {
        return Some(clip.to_string());
    }
    None
}
