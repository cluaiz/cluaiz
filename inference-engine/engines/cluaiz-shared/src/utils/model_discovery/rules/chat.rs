use crate::utils::ModelCapabilities;

pub fn evaluate_chat_rules(
    arch_lower: &str,
    has_chat_template: bool,
    caps: &mut ModelCapabilities,
) {
    if has_chat_template || arch_lower.contains("instruct") || arch_lower.contains("chat") || arch_lower.contains("-it") {
        caps.is_instruct = true;
    } else {
        caps.is_base = true;
    }

    if arch_lower.contains("coder") || arch_lower.contains("starcoder") || arch_lower.contains("deepseek-coder") {
        caps.has_file = true;
    }
}

pub fn get_chat_tasks(caps: &ModelCapabilities) -> Vec<String> {
    let mut tasks = vec![];
    if caps.is_base {
        // "text-generation": Applied strictly when the model is a raw base completion model.
        tasks.push("text-generation".to_string());
    }
    if caps.is_instruct {
        // "chat-completion": Applied strictly when the model is instruction-tuned/RLHF-aligned for multi-turn chat.
        tasks.push("chat-completion".to_string());
    }
    if tasks.is_empty() {
        tasks.push("chat-completion".to_string());
    }
    if caps.has_vision {
        // "multimodal-vision": Applied when the model contains vision encoder/projector weights to process image inputs alongside text.
        tasks.push("multimodal-vision".to_string());
    }
    if caps.has_video {
        // "multimodal-video": Applied when the model contains temporal attention/frame-processing layers for video sequence understanding.
        tasks.push("multimodal-video".to_string());
    }
    if caps.has_file {
        // "multimodal-file": Applied when the model natively parses and contextualizes structured file documents (PDF, raw codebases, JSON).
        tasks.push("multimodal-file".to_string());
    }
    if caps.has_audio {
        // "multimodal-audio": Applied when the chat model directly processes or responds with speech/audio spectrogram tensors.
        tasks.push("multimodal-audio".to_string());
    }
    tasks
}
