use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct VotingArbitrator;

impl VotingArbitrator {
    /// Normalizes different task strings to standard Cluaiz tasks
    fn normalize_task(raw_task: &str) -> String {
        let t = raw_task.to_lowercase().replace("-", "_");
        match t.as_str() {
            "automatic_speech_recognition" | "asr" | "speech_recognition" => "speech_to_text".to_string(),
            "tts" | "text_to_speech" | "text_to_audio" => "text_to_speech".to_string(),
            "voice_conversion" | "audio_to_audio" => "voice_conversion".to_string(),
            "image_to_text" | "image_text_to_text" | "vqa" | "visual_question_answering" => "multimodal_vision_chat".to_string(),
            "text_generation" | "chat" | "conversational" => "chat_completion".to_string(),
            "feature_extraction" | "sentence_similarity" => "embedding".to_string(),
            _ => raw_task.to_string(),
        }
    }

    /// Performs the 3-Way Vote to determine the actual tasks for the model
    pub fn resolve_tasks(
        model_dir: &Path,
        header_tasks: &[String],
        arch_lower: &str,
    ) -> Vec<String> {
        let mut scores: HashMap<String, u32> = HashMap::new();

        // SOURCE 1: ONNX Header (Weight: 30)
        for t in header_tasks {
            let norm = Self::normalize_task(t);
            *scores.entry(norm).or_insert(0) += 30;
        }

        // SOURCE 2: HF API Metadata (Weight: 45)
        let hf_meta_path = model_dir.join("hf_metadata.json");
        if hf_meta_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&hf_meta_path) {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    if let Some(pipeline_tag) = json.get("pipeline_tag").and_then(|v| v.as_str()) {
                        let norm = Self::normalize_task(pipeline_tag);
                        *scores.entry(norm).or_insert(0) += 45;
                    }
                    if let Some(tags) = json.get("tags").and_then(|v| v.as_array()) {
                        for tag in tags {
                            if let Some(tag_str) = tag.as_str() {
                                let norm = Self::normalize_task(tag_str);
                                // Tags get a slight boost, but not as much as the primary pipeline tag
                                *scores.entry(norm).or_insert(0) += 10;
                            }
                        }
                    }
                }
            }
        }

        // SOURCE 3: Filesystem & Name Heuristics (Weight: 25)
        let dir_name = model_dir.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
        let combined_ident = format!("{} {}", arch_lower, dir_name);

        if model_dir.join("tts.json").exists() 
            || combined_ident.contains("tts") 
            || combined_ident.contains("kokoro")
            || combined_ident.contains("vits") 
            || combined_ident.contains("vocoder")
            || combined_ident.contains("supertonic")
            || combined_ident.contains("bark")
            || combined_ident.contains("cosyvoice")
            || combined_ident.contains("chattts")
            || combined_ident.contains("parler")
            || combined_ident.contains("fastspeech")
            || combined_ident.contains("piper")
        {
            *scores.entry(Self::normalize_task("text_to_speech")).or_insert(0) += 25;
        }
        
        if combined_ident.contains("whisper") {
            *scores.entry(Self::normalize_task("speech_to_text")).or_insert(0) += 25;
        }

        if combined_ident.contains("conversion") || combined_ident.contains("demucs") {
            *scores.entry(Self::normalize_task("voice_conversion")).or_insert(0) += 25;
        }

        if combined_ident.contains("bert") || combined_ident.contains("embedding") || combined_ident.contains("bge") || combined_ident.contains("nomic") {
            *scores.entry(Self::normalize_task("embedding")).or_insert(0) += 25;
        }

        if combined_ident.contains("instruct") || combined_ident.contains("chat") || combined_ident.contains("-it") {
            *scores.entry(Self::normalize_task("chat_completion")).or_insert(0) += 25;
        }

        // Gather final tasks that pass a threshold or simply the highest voted ones
        let mut final_tasks = Vec::new();
        let mut max_score = 0;
        
        for &score in scores.values() {
            if score > max_score {
                max_score = score;
            }
        }

        // Any task that scored at least 40 or is the absolute max score
        for (task, &score) in &scores {
            if score >= 40 || (score == max_score && max_score > 0) {
                // Map multimodal chat tasks correctly
                if task == "multimodal_vision_chat" {
                    final_tasks.push("multimodal-vision".to_string());
                    final_tasks.push("chat-completion".to_string());
                } else if task == "chat_completion" {
                    final_tasks.push("chat-completion".to_string());
                } else if task == "speech_to_text" {
                    final_tasks.push("speech_to_text".to_string());
                } else if task == "text_to_speech" {
                    final_tasks.push("text_to_speech".to_string());
                } else if task == "embedding" {
                    final_tasks.push("embedding".to_string());
                } else {
                    final_tasks.push(task.clone());
                }
            }
        }
        
        // Remove duplicates
        final_tasks.sort();
        final_tasks.dedup();

        final_tasks
    }
}
