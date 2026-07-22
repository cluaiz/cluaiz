use crate::utils::ModelCapabilities;

pub fn evaluate_vision_rules(
    arch_lower: &str,
    has_vision_keys: bool,
    has_vision_tensors: bool,
    caps: &mut ModelCapabilities,
) {
    let is_qwen_vl = arch_lower.contains("qwen") && arch_lower.contains("vl");
    let is_gemma4 = arch_lower.contains("gemma4") || arch_lower.contains("gemma-4");

    if has_vision_keys
        || has_vision_tensors
        || is_qwen_vl
        || arch_lower.contains("llava")
        || arch_lower.contains("phi3v")
        || is_gemma4
    {
        caps.has_vision = true;
        if caps.is_instruct {
            caps.is_vision_chat = true;
        }
        if arch_lower.contains("vqa") || arch_lower.contains("pali") {
            caps.is_vqa = true;
        }
        if arch_lower.contains("ocr")
            || arch_lower.contains("caption")
            || arch_lower.contains("nougat")
            || arch_lower.contains("surya")
        {
            caps.is_image_to_text = true;
        }
    }

    if arch_lower.contains("diffusion")
        || arch_lower.contains("flux")
        || arch_lower.contains("sdxl")
        || arch_lower.contains("pixart")
    {
        caps.is_image_gen = true;
    }

    if arch_lower.contains("cogvideo")
        || arch_lower.contains("svd")
        || arch_lower.contains("animatediff")
    {
        caps.is_video_gen = true;
        caps.has_video = true;
    }
}

pub fn get_vision_tasks(caps: &ModelCapabilities) -> Vec<String> {
    let mut tasks = vec![];
    if caps.is_vision_chat {
        // "vision-chat": Applied when a dedicated vision-first model is optimized for interactive visual dialogue.
        tasks.push("vision-chat".to_string());
    }
    if caps.has_vision && !caps.is_instruct {
        tasks.push("vision-feature-extraction".to_string());
    }
    if caps.is_image_to_text {
        // "image-to-text": Applied for dedicated image captioning or optical character recognition (OCR) models.
        tasks.push("image-to-text".to_string());
    }
    if caps.is_vqa {
        // "visual-question-answering": Applied when the model is specialized for answering questions grounded in input images.
        tasks.push("visual-question-answering".to_string());
    }
    if caps.is_image_gen {
        // "image-generation": Applied for diffusion or autoregressive image generation models (text/image to image output).
        tasks.push("image-generation".to_string());
    }
    if caps.is_video_gen || caps.has_video {
        // "video-generation": Applied for video diffusion/generation models (text/image to video output).
        tasks.push("video-generation".to_string());
    }
    tasks
}
