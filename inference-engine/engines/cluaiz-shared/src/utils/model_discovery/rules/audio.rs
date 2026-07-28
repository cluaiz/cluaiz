use crate::utils::ModelCapabilities;

pub fn evaluate_audio_rules(
    arch_lower: &str,
    has_audio_keys: bool,
    has_audio_tensors: bool,
    caps: &mut ModelCapabilities,
) {
    let is_gemma4 = arch_lower.contains("gemma4") || arch_lower.contains("gemma-4");
    if has_audio_keys
        || has_audio_tensors
        || arch_lower.contains("whisper")
        || arch_lower.contains("bark")
        || arch_lower.contains("kokoro")
        || arch_lower.contains("cosyvoice")
        || arch_lower.contains("chattts")
        || arch_lower.contains("parler")
        || arch_lower.contains("fastspeech")
        || arch_lower.contains("tts")
        || is_gemma4
    {
        caps.has_audio = true;
        if arch_lower.contains("whisper") {
            caps.is_asr = true;
        }
        if arch_lower.contains("bark")
            || arch_lower.contains("piper")
            || arch_lower.contains("vits")
            || arch_lower.contains("kokoro")
            || arch_lower.contains("cosyvoice")
            || arch_lower.contains("chattts")
            || arch_lower.contains("parler")
            || arch_lower.contains("fastspeech")
            || arch_lower.contains("tts")
        {
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

fn normalize_audio_task(task: &str) -> String {
    match task.to_lowercase().replace("-", "_").as_str() {
        "automatic_speech_recognition" | "asr" | "speech_recognition" => "speech_to_text".to_string(),
        "speech_translation" | "translation" => "speech_translation".to_string(),
        "tts" | "text_to_speech" => "text_to_speech".to_string(),
        "voice_conversion" | "audio_to_audio" => "voice_conversion".to_string(),
        "audio_classification" | "sound_classification" => "audio_classification".to_string(),
        "speaker_identification" | "speaker_id" => "speaker_identification".to_string(),
        "speaker_verification" => "speaker_verification".to_string(),
        "speaker_diarization" | "diarization" => "speaker_diarization".to_string(),
        "emotion_recognition" | "speech_emotion_recognition" => "emotion_recognition".to_string(),
        "language_identification" | "lang_id" => "language_identification".to_string(),
        "keyword_spotting" | "kws" => "keyword_spotting".to_string(),
        "wake_word_detection" | "wake_word" => "wake_word_detection".to_string(),
        "voice_activity_detection" | "vad" => "voice_activity_detection".to_string(),
        "noise_reduction" | "denoiser" => "noise_reduction".to_string(),
        "source_separation" | "speech_separation" => "source_separation".to_string(),
        "audio_enhancement" | "speech_enhancement" => "audio_enhancement".to_string(),
        "music_generation" | "text_to_music" => "music_generation".to_string(),
        "music_classification" => "music_classification".to_string(),
        "audio_embedding" | "feature_extraction" => "audio_embedding".to_string(),
        "audio_captioning" => "audio_captioning".to_string(),
        other => other.to_string(),
    }
}

pub fn get_audio_tasks(caps: &ModelCapabilities) -> Vec<String> {
    if !caps.explicit_tasks.is_empty() {
        return caps
            .explicit_tasks
            .iter()
            .map(|t| normalize_audio_task(t))
            .collect();
    }
    let mut tasks = vec![];
    if caps.is_asr {
        tasks.push("speech_to_text".to_string());
    }
    if caps.is_tts {
        tasks.push("text_to_speech".to_string());
    }
    if caps.is_audio_to_audio {
        tasks.push("voice_conversion".to_string());
    }
    if caps.is_audio_class {
        tasks.push("audio_classification".to_string());
    }
    if tasks.is_empty() {
        tasks.push("speech_to_text".to_string());
    }
    tasks
}

// -># task Enum:->
// speech_to_text
// speech_translation
// text_to_speech
// voice_conversion
// audio_classification
// speaker_identification
// speaker_verification
// speaker_diarization
// emotion_recognition
// language_identification
// keyword_spotting
// wake_word_detection
// voice_activity_detection
// noise_reduction
// source_separation
// audio_enhancement
// music_generation
// music_classification
// audio_embedding
// audio_captioning
