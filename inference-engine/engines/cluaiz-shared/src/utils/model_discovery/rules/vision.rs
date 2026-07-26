use crate::utils::ModelCapabilities;

pub fn evaluate_vision_rules(
    _arch_lower: &str,
    has_vision_keys: bool,
    has_vision_tensors: bool,
    caps: &mut ModelCapabilities,
) {
    if has_vision_keys || has_vision_tensors {
        caps.has_vision = true;
        if caps.is_instruct {
            caps.is_vision_chat = true;
        }
    }
}

fn normalize_vision_task(task: &str) -> String {
    match task.to_lowercase().replace("-", "_").as_str() {
        "visual_question_answering" | "vqa" => "visual-question-answering".to_string(),
        "doc_vqa" | "document_question_answering" | "document_understanding" | "visual_document_understanding" => "document-question-answering".to_string(),
        "multimodal_dialogue" | "visual_dialogue" | "vision_chat" | "multimodal_chat" | "vlm" => "multimodal-dialogue".to_string(),
        "image_to_text" | "captioning" | "image_captioning" => "image-to-text".to_string(),
        "ocr" | "optical_character_recognition" | "document_parsing" => "optical-character-recognition".to_string(),
        "image_classification" | "vision_feature_extraction" | "feature_extraction" | "image_embedding" => "vision-feature-extraction".to_string(),
        "zero_shot_image_classification" | "clip" => "zero-shot-image-classification".to_string(),
        "object_detection" | "detection" => "object-detection".to_string(),
        "zero_shot_object_detection" | "grounding" => "zero-shot-object-detection".to_string(),
        "pose_estimation" | "keypoint_detection" => "pose-estimation".to_string(),
        "object_tracking" | "video_tracking" => "object-tracking".to_string(),
        "image_segmentation" | "segmentation" | "mask_generation" | "sam" => "image-segmentation".to_string(),
        "depth_estimation" | "monocular_depth" => "depth-estimation".to_string(),
        "text_to_image" | "image_generation" | "diffusion" => "text-to-image".to_string(),
        "image_to_image" | "img2img" => "image-to-image".to_string(),
        "image_inpainting" | "inpainting" => "image-inpainting".to_string(),
        "super_resolution" | "image_upscaling" | "upscaling" => "super-resolution".to_string(),
        "text_to_video" | "video_generation" => "text-to-video".to_string(),
        "image_to_video" | "img2vid" => "image-to-video".to_string(),
        "video_to_video" | "vid2vid" => "video-to-video".to_string(),
        "video_classification" | "action_recognition" => "video-classification".to_string(),
        "video_captioning" | "video_to_text" => "video-captioning".to_string(),
        "3d_reconstruction" | "nerf" | "gaussian_splatting" => "3d-reconstruction".to_string(),
        other => other.replace("_", "-"),
    }
}

pub fn get_vision_tasks(caps: &ModelCapabilities) -> Vec<String> {
    if !caps.explicit_tasks.is_empty() {
        return caps
            .explicit_tasks
            .iter()
            .map(|t| normalize_vision_task(t))
            .collect();
    }
    let mut tasks = vec![];

    if caps.is_vision_chat || (caps.has_vision && caps.is_instruct) {
        tasks.push("multimodal-dialogue".to_string());
    }
    if caps.is_image_to_text {
        tasks.push("image-to-text".to_string());
    }
    if caps.is_vqa {
        tasks.push("visual-question-answering".to_string());
    }
    if caps.is_image_gen {
        tasks.push("text-to-image".to_string());
    }
    if caps.is_video_gen || caps.has_video {
        tasks.push("text-to-video".to_string());
    }

    if tasks.is_empty() && caps.has_vision {
        tasks.push("vision-feature-extraction".to_string());
    }

    tasks
}
