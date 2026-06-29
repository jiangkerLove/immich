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

#[derive(Debug, Clone)]
pub struct DetectedFace {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub embedding: String,
}

#[derive(Debug, Clone)]
pub struct FaceDetectionResult {
    pub image_width: i32,
    pub image_height: i32,
    pub faces: Vec<DetectedFace>,
}

#[derive(Debug, Clone)]
pub struct OcrResult {
    pub text: Vec<String>,
    pub r#box: Vec<f32>,
    pub box_score: Vec<f32>,
    pub text_score: Vec<f32>,
}

pub async fn detect_faces(
    config: &MachineLearningConfig,
    image_path: &Path,
    model_name: &str,
    min_score: f64,
) -> Result<FaceDetectionResult, ErrorResp> {
    let entries = serde_json::json!({
        "facial-recognition": {
            "detection": { "modelName": model_name, "options": { "minScore": min_score } },
            "recognition": { "modelName": model_name }
        }
    });

    let body = predict_image(config, image_path, &entries).await?;
    let image_width = body
        .get("imageWidth")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    let image_height = body
        .get("imageHeight")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;

    let faces = body
        .get("facial-recognition")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|face| {
                    let bbox = face.get("boundingBox")?;
                    Some(DetectedFace {
                        x1: bbox.get("x1")?.as_f64()?,
                        y1: bbox.get("y1")?.as_f64()?,
                        x2: bbox.get("x2")?.as_f64()?,
                        y2: bbox.get("y2")?.as_f64()?,
                        embedding: face.get("embedding")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(FaceDetectionResult {
        image_width,
        image_height,
        faces,
    })
}

pub async fn run_ocr(
    config: &MachineLearningConfig,
    image_path: &Path,
    ocr: &crate::models::db::system_metadata::OcrConfig,
) -> Result<OcrResult, ErrorResp> {
    let entries = serde_json::json!({
        "ocr": {
            "detection": {
                "modelName": ocr.model_name,
                "options": {
                    "minScore": ocr.min_detection_score,
                    "maxResolution": ocr.max_resolution
                }
            },
            "recognition": {
                "modelName": ocr.model_name,
                "options": { "minScore": ocr.min_recognition_score }
            }
        }
    });

    let body = predict_image(config, image_path, &entries).await?;
    let ocr_value = body.get("ocr").cloned().unwrap_or_default();

    Ok(OcrResult {
        text: ocr_value
            .get("text")
            .and_then(|v| {
                v.as_array().map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
            })
            .unwrap_or_default(),
        r#box: json_number_array(&ocr_value, "box"),
        box_score: json_number_array(&ocr_value, "boxScore"),
        text_score: json_number_array(&ocr_value, "textScore"),
    })
}

async fn predict_image(
    config: &MachineLearningConfig,
    image_path: &Path,
    entries: &serde_json::Value,
) -> Result<serde_json::Value, ErrorResp> {
    let bytes = tokio::fs::read(image_path)
        .await
        .map_err(|e| ErrorResp::ServerError(format!("Failed to read image for ML: {e}")))?;
    let file_name = image_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image.jpg")
        .to_string();

    let client = reqwest::Client::new();
    for url in &config.urls {
        let endpoint = format!("{}/predict", url.trim_end_matches('/'));
        let image_part = reqwest::multipart::Part::bytes(bytes.clone())
            .file_name(file_name.clone())
            .mime_str("application/octet-stream")
            .map_err(|e| ErrorResp::ServerError(e.to_string()))?;
        let form = reqwest::multipart::Form::new()
            .text("entries", entries.to_string())
            .part("image", image_part);

        match client.post(&endpoint).multipart(form).send().await {
            Ok(response) if response.status().is_success() => {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    return Ok(body);
                }
            }
            _ => continue,
        }
    }

    Err(ErrorResp::ServerError(
        "Machine learning image prediction failed".to_string(),
    ))
}

fn json_number_array(value: &serde_json::Value, key: &str) -> Vec<f32> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_f64().map(|n| n as f32))
                .collect()
        })
        .unwrap_or_default()
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
