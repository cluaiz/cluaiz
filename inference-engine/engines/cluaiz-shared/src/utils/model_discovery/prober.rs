use std::path::Path;
use crate::utils::{GGUFProber, RegistryModelMetadata};

pub struct BinaryProbeResult {
    pub architecture: String,
    pub context_window: String,
    pub parameters_str: String,
    pub quantization: Option<String>,
    pub bit_depth: Option<String>,
    pub chat_template: Option<String>,
    pub think_start_tag: Option<String>,
    pub think_end_tag: Option<String>,
    pub requires_gpu: bool,
    pub has_vision_keys: bool,
    pub has_vision_tensors: bool,
    pub has_audio_keys: bool,
    pub has_audio_tensors: bool,
    pub has_pooling: bool,
}

pub fn probe_weight_binary(weight_path: &Path, format_type: &str) -> BinaryProbeResult {
    let file_size_bytes = weight_path.metadata().map_or(0, |m| m.len());

    let mut res = BinaryProbeResult {
        architecture: "Unknown".to_string(),
        context_window: "Unknown".to_string(),
        parameters_str: "Unknown".to_string(),
        quantization: None,
        bit_depth: None,
        chat_template: None,
        think_start_tag: None,
        think_end_tag: None,
        requires_gpu: false, // GGUF and ONNX models support CPU Execution Provider
        has_vision_keys: false,
        has_vision_tensors: false,
        has_audio_keys: false,
        has_audio_tensors: false,
        has_pooling: false,
    };

    if format_type == "gguf" {
        if let Ok((metadata, tensor_infos, _)) = GGUFProber::probe(weight_path) {
            // 1. Architecture & Context Length
            if let Some(arch) = metadata.get("general.architecture") {
                res.architecture = arch.clone();
                if let Some(ctx) = metadata.get(&format!("{}.context_length", arch)) {
                    res.context_window = ctx.clone();
                }
            }

            // 2. Quantization & Bit Depth Extraction
            if let Some(quant) = metadata.get("general.file_type") {
                res.quantization = Some(quant.clone());
            }
            if let Some(ftype) = metadata.get("general.quantization_version") {
                let depth = match ftype.as_str() {
                    "1" | "Q2_K" => "2-bit",
                    "2" | "Q4_0" | "Q4_1" | "Q4_K_M" | "Q4_K_S" => "4-bit",
                    "3" | "Q8_0" => "8-bit",
                    "0" | "F16" => "16-bit",
                    _ => "4-bit",
                };
                res.bit_depth = Some(depth.to_string());
            }

            // 3. Compute parameter count
            let mut total_params: u64 = 0;
            for (_name, dims) in &tensor_infos {
                if !dims.is_empty() {
                    let count: u64 = dims.iter().map(|&d| d as u64).product();
                    total_params += count;
                }
            }
            if total_params > 0 {
                res.parameters_str = format!("{:.2}B", total_params as f64 / 1_000_000_000.0);
            }

            // 4. Header Binary Chat Template & Reasoning Tag Extraction
            if let Some(tmpl) = metadata.get("tokenizer.chat_template") {
                res.chat_template = Some(tmpl.clone());
                let (start, end) = find_header_reasoning_tags(tmpl);
                res.think_start_tag = start;
                res.think_end_tag = end;
            }

            // 5. Pooling & Namespaces
            res.has_pooling = metadata.contains_key("general.pooling_type");
            res.has_vision_keys = metadata.keys().any(|k| k.starts_with("clip.vision.") || k.starts_with("vision.") || k.starts_with("llava."));
            res.has_vision_tensors = tensor_infos.iter().any(|(name, _)| name.contains("visual") || name.contains("mm_projector") || name.contains("v.proj"));
            res.has_audio_keys = metadata.keys().any(|k| k.starts_with("whisper.") || k.starts_with("audio.") || k.starts_with("bark."));
            res.has_audio_tensors = tensor_infos.iter().any(|(name, _)| name.contains("audio_encoder") || name.contains("mel_filters") || name.contains("speech"));
        }
    } else if format_type == "onnx" {
        // ONNX Prober Branch
        let file_stem = weight_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let parent_dir = weight_path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

        // Architecture Detection for ONNX
        if file_stem.contains("bge") || parent_dir.contains("bge") {
            res.architecture = "bge".to_string();
            res.has_pooling = true;
        } else if file_stem.contains("nomic") || parent_dir.contains("nomic") {
            res.architecture = "nomic".to_string();
            res.has_pooling = true;
        } else if file_stem.contains("clip") || parent_dir.contains("clip") {
            res.architecture = "clip".to_string();
            res.has_vision_keys = true;
            res.has_vision_tensors = true;
        } else if file_stem.contains("whisper") || parent_dir.contains("whisper") {
            res.architecture = "whisper".to_string();
            res.has_audio_keys = true;
            res.has_audio_tensors = true;
        } else if file_stem.contains("kokoro") || parent_dir.contains("kokoro") || file_stem.contains("tts") || parent_dir.contains("tts") || file_stem.contains("bark") || parent_dir.contains("bark") {
            res.architecture = "kokoro".to_string();
            res.has_audio_keys = true;
            res.has_audio_tensors = true;
        } else if file_stem.contains("bert") || parent_dir.contains("bert") {
            res.architecture = "bert".to_string();
            res.has_pooling = true;
        } else {
            res.architecture = parent_dir;
        }

        // Quantization detection from ONNX filename hints
        if file_stem.contains("int8") || file_stem.contains("q8") {
            res.quantization = Some("INT8".to_string());
            res.bit_depth = Some("8-bit".to_string());
        } else if file_stem.contains("fp16") || file_stem.contains("q4") {
            res.quantization = Some("FP16".to_string());
            res.bit_depth = Some("16-bit".to_string());
        } else {
            res.quantization = Some("FP32".to_string());
            res.bit_depth = Some("32-bit".to_string());
        }
    }

    res
}

pub fn find_header_reasoning_tags(tmpl: &str) -> (Option<String>, Option<String>) {
    if tmpl.contains("<think>") {
        return (Some("<think>".to_string()), Some("</think>".to_string()));
    }
    if tmpl.contains("<|think|>") {
        let end = if tmpl.contains("<channel|>") {
            "<channel|>".to_string()
        } else if tmpl.contains("<|think_end|>") {
            "<|think_end|>".to_string()
        } else {
            "</think>".to_string()
        };
        return (Some("<|think|>".to_string()), Some(end));
    }
    if tmpl.contains("<|channel>thought") || tmpl.contains("<channel|>thought") {
        return (Some("<|channel>thought".to_string()), Some("<channel|>".to_string()));
    }
    if tmpl.contains("<thought>") {
        return (Some("<thought>".to_string()), Some("</thought>".to_string()));
    }
    if tmpl.contains("<reasoning>") {
        return (Some("<reasoning>".to_string()), Some("</reasoning>".to_string()));
    }

    (None, None)
}
