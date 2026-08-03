use crate::utils::ModelCapabilities;

pub fn evaluate_embedding_rules(
    arch_lower: &str,
    has_pooling: bool,
    caps: &mut ModelCapabilities,
) {
    // 🗳️ Use Arbitrator's Confident Vote
    if caps.explicit_tasks.contains(&"embedding".to_string()) || caps.explicit_tasks.contains(&"feature-extraction".to_string()) {
        caps.is_embedding = true;
        caps.is_feature_extraction = true;
    } else {
        // Fallback for old cases where tasks aren't known
        let is_embedding_arch = has_pooling 
            || arch_lower.contains("bert")
            || arch_lower.contains("nomic")
            || arch_lower.contains("bge")
            || arch_lower.contains("gte")
            || arch_lower.contains("e5")
            || arch_lower.contains("minilm");

        if is_embedding_arch {
            caps.is_embedding = true;
            caps.is_feature_extraction = true;
        }
    }
}

pub fn get_embedding_tasks(caps: &ModelCapabilities) -> Vec<String> {
    let mut tasks = vec![];
    if caps.is_embedding || tasks.is_empty() {
        // "embedding": Applied strictly when the model generates dense or sparse text vector embeddings via pooled encoder hidden states.
        tasks.push("embedding".to_string());
    }
    if caps.is_feature_extraction {
        // "feature-extraction": Applied when the model extracts raw intermediate hidden-layer representations for downstream tasks.
        tasks.push("feature-extraction".to_string());
    }
    if caps.has_vision {
        // "vision-embedding": Applied when the model projects image inputs into a shared vector embedding space (e.g. CLIP).
        tasks.push("vision-embedding".to_string());
    }
    if caps.has_audio {
        // "audio-embedding": Applied when the model projects audio waveform/spectrogram inputs into vector embedding space.
        tasks.push("audio-embedding".to_string());
    }
    tasks
}
