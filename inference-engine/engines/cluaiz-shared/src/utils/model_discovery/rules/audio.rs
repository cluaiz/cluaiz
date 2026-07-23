use crate::utils::ModelCapabilities;

pub fn evaluate_audio_rules(
    arch_lower: &str,
    has_audio_keys: bool,
    has_audio_tensors: bool,
    caps: &mut ModelCapabilities,
) {
    let is_gemma4 = arch_lower.contains("gemma4") || arch_lower.contains("gemma-4");
    if has_audio_keys || has_audio_tensors || arch_lower.contains("whisper") || arch_lower.contains("bark") || arch_lower.contains("kokoro") || arch_lower.contains("tts") || is_gemma4 {
        caps.has_audio = true;
        if arch_lower.contains("whisper") {
            caps.is_asr = true;
        }
        if arch_lower.contains("bark") || arch_lower.contains("piper") || arch_lower.contains("vits") || arch_lower.contains("kokoro") {
            caps.is_tts = true;
        }
        if arch_lower.contains("conversion") || arch_lower.contains("demucs") {
            caps.is_audio_to_audio = true;
        }
        if arch_lower.contains("clap") || arch_lower.contains("ast") {
            caps.is_audio_class = true;
        }
    }
}

pub fn get_audio_tasks(caps: &ModelCapabilities) -> Vec<String> {
    let mut tasks = vec![];
    if caps.is_asr {
        // "automatic-speech-recognition": Applied for speech-to-text transcription models (e.g. Whisper).
        tasks.push("automatic-speech-recognition".to_string());
    }
    if caps.is_tts || (tasks.is_empty() && !caps.is_asr) {
        // "text-to-speech": Applied for text-to-speech synthesis models (e.g. Bark, Coqui, Kokoro).
        tasks.push("text-to-speech".to_string());
    }
    if caps.is_audio_to_audio {
        // "audio-to-audio": Applied for audio translation, voice conversion, or speech enhancement models.
        tasks.push("audio-to-audio".to_string());
    }
    if caps.is_audio_class {
        // "audio-classification": Applied for acoustic event detection or sound classification models.
        tasks.push("audio-classification".to_string());
    }
    tasks
}
