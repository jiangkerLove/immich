use std::collections::HashMap;
use std::sync::OnceLock;

static CLIP_MODELS: OnceLock<HashMap<&'static str, i32>> = OnceLock::new();

fn clip_models() -> &'static HashMap<&'static str, i32> {
    CLIP_MODELS.get_or_init(|| {
        [
            ("RN101__openai", 512),
            ("RN101__yfcc15m", 512),
            ("ViT-B-16__laion400m_e31", 512),
            ("ViT-B-16__laion400m_e32", 512),
            ("ViT-B-16__openai", 512),
            ("ViT-B-32__laion2b-s34b-b79k", 512),
            ("ViT-B-32__laion2b_e16", 512),
            ("ViT-B-32__laion400m_e31", 512),
            ("ViT-B-32__laion400m_e32", 512),
            ("ViT-B-32__openai", 512),
            ("XLM-Roberta-Base-ViT-B-32__laion5b_s13b_b90k", 512),
            ("XLM-Roberta-Large-Vit-B-32", 512),
            ("RN50x4__openai", 640),
            ("ViT-B-16-plus-240__laion400m_e31", 640),
            ("ViT-B-16-plus-240__laion400m_e32", 640),
            ("XLM-Roberta-Large-Vit-B-16Plus", 640),
            ("LABSE-Vit-L-14", 768),
            ("RN50x16__openai", 768),
            ("ViT-B-16-SigLIP-256__webli", 768),
            ("ViT-B-16-SigLIP-384__webli", 768),
            ("ViT-B-16-SigLIP-512__webli", 768),
            ("ViT-B-16-SigLIP-i18n-256__webli", 768),
            ("ViT-B-16-SigLIP__webli", 768),
            ("ViT-L-14-336__openai", 768),
            ("ViT-L-14-quickgelu__dfn2b", 768),
            ("ViT-L-14__laion2b-s32b-b82k", 768),
            ("ViT-L-14__laion400m_e31", 768),
            ("ViT-L-14__laion400m_e32", 768),
            ("ViT-L-14__openai", 768),
            ("XLM-Roberta-Large-Vit-L-14", 768),
            ("nllb-clip-base-siglip__mrl", 768),
            ("nllb-clip-base-siglip__v1", 768),
            ("RN50__cc12m", 1024),
            ("RN50__openai", 1024),
            ("RN50__yfcc15m", 1024),
            ("RN50x64__openai", 1024),
            ("ViT-H-14-378-quickgelu__dfn5b", 1024),
            ("ViT-H-14-quickgelu__dfn5b", 1024),
            ("ViT-H-14__laion2b-s32b-b79k", 1024),
            ("ViT-L-16-SigLIP-256__webli", 1024),
            ("ViT-L-16-SigLIP-384__webli", 1024),
            ("ViT-g-14__laion2b-s12b-b42k", 1024),
            ("XLM-Roberta-Large-ViT-H-14__frozen_laion5b_s13b_b90k", 1024),
            ("ViT-SO400M-14-SigLIP-384__webli", 1152),
            ("nllb-clip-large-siglip__mrl", 1152),
            ("nllb-clip-large-siglip__v1", 1152),
            ("ViT-B-16-SigLIP2__webli", 768),
            ("ViT-B-32-SigLIP2-256__webli", 768),
            ("ViT-L-16-SigLIP2-256__webli", 1024),
            ("ViT-L-16-SigLIP2-384__webli", 1024),
            ("ViT-L-16-SigLIP2-512__webli", 1024),
            ("ViT-SO400M-14-SigLIP2__webli", 1152),
            ("ViT-SO400M-14-SigLIP2-378__webli", 1152),
            ("ViT-SO400M-16-SigLIP2-256__webli", 1152),
            ("ViT-SO400M-16-SigLIP2-384__webli", 1152),
            ("ViT-SO400M-16-SigLIP2-512__webli", 1152),
            ("ViT-gopt-16-SigLIP2-256__webli", 1536),
            ("ViT-gopt-16-SigLIP2-384__webli", 1536),
        ]
        .into_iter()
        .collect()
    })
}

fn clean_model_name(model_name: &str) -> String {
    model_name
        .split('/')
        .next_back()
        .unwrap_or(model_name)
        .replace(':', "_")
}

pub fn get_clip_dim_size(model_name: &str) -> Result<i32, String> {
    let cleaned = clean_model_name(model_name);
    clip_models()
        .get(cleaned.as_str())
        .copied()
        .ok_or_else(|| format!("Unknown CLIP model: {model_name}"))
}
