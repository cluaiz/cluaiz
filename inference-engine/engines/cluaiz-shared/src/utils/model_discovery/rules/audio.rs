use crate::utils::ModelCapabilities;

pub fn evaluate_audio_rules(
    arch_lower: &str,
    has_audio_keys: bool,
    has_audio_tensors: bool,
    caps: &mut ModelCapabilities,
) {
    // 🗳️ Use Arbitrator's Confident Vote
    let has_audio_task = caps.explicit_tasks.contains(&"text_to_speech".to_string())
        || caps.explicit_tasks.contains(&"speech_to_text".to_string())
        || caps.explicit_tasks.contains(&"voice_conversion".to_string())
        || caps.explicit_tasks.contains(&"audio_classification".to_string());

    if has_audio_task || has_audio_keys || has_audio_tensors {
        caps.has_audio = true;
        
        if caps.explicit_tasks.contains(&"speech_to_text".to_string()) {
            caps.is_asr = true;
        } else if caps.explicit_tasks.contains(&"text_to_speech".to_string()) {
            caps.is_tts = true;
        } else if caps.explicit_tasks.contains(&"voice_conversion".to_string()) {
            caps.is_audio_to_audio = true;
        } else if caps.explicit_tasks.contains(&"audio_classification".to_string()) {
            caps.is_audio_class = true;
        } else {
            // Fallback for cases where tasks aren't known but keys are present
            if arch_lower.contains("whisper") {
                caps.is_asr = true;
            } else {
                caps.is_tts = true;
            }
        }

        // Detect explicit TTS / Audio Model Family
        if arch_lower.contains("kokoro") {
            caps.tts_family = Some("kokoro".to_string());
        } else if arch_lower.contains("supertonic") || arch_lower.contains("luxtts") {
            caps.tts_family = Some("supertonic".to_string());
        } else if arch_lower.contains("matcha") {
            caps.tts_family = Some("cosyvoice_matcha".to_string());
        } else if arch_lower.contains("vits") || arch_lower.contains("piper") {
            caps.tts_family = Some("vits_piper".to_string());
        } else if arch_lower.contains("cosyvoice") {
            caps.tts_family = Some("cosyvoice_matcha".to_string());
        } else if arch_lower.contains("audio8") {
            caps.tts_family = Some("audio8".to_string());
        } else if arch_lower.contains("chatterbox") {
            caps.tts_family = Some("chatterbox".to_string());
        } else if arch_lower.contains("whisper") {
            caps.tts_family = Some("whisper".to_string());
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
