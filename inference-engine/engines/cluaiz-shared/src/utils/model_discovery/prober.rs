use crate::utils::{GGUFProber, RegistryModelMetadata};
use std::path::Path;

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
    pub explicit_tasks: Vec<String>,
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
        explicit_tasks: vec![],
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

            if let Some(tag) = metadata.get("general.pipeline_tag") {
                res.explicit_tasks.push(tag.clone());
            } else if let Some(tag) = metadata.get("general.task") {
                res.explicit_tasks.push(tag.clone());
            }

            // 2. Quantization & Bit Depth Extraction
            let mut detected_quant = metadata.get("general.file_type").cloned();
            
            // Map GGUF general.file_type numeric enum to human string if applicable
            if let Some(ref q) = detected_quant {
                let human_quant = match q.as_str() {
                    "0" => "ALL_F32",
                    "1" => "MOSTLY_F16",
                    "2" => "MOSTLY_Q4_0",
                    "3" => "MOSTLY_Q4_1",
                    "7" => "MOSTLY_Q8_0",
                    "8" => "MOSTLY_Q5_0",
                    "9" => "MOSTLY_Q5_1",
                    "10" => "MOSTLY_Q2_K",
                    "11" => "MOSTLY_Q3_K_S",
                    "12" => "MOSTLY_Q3_K_M",
                    "13" => "MOSTLY_Q3_K_L",
                    "14" => "MOSTLY_Q4_K_S",
                    "15" => "MOSTLY_Q4_K_M",
                    "16" => "MOSTLY_Q5_K_S",
                    "17" => "MOSTLY_Q5_K_M",
                    "18" => "MOSTLY_Q6_K",
                    "24" => "MOSTLY_IQ2_XXS",
                    "25" => "MOSTLY_IQ2_XS",
                    "26" => "MOSTLY_IQ2_S",
                    "27" => "MOSTLY_IQ3_XXS",
                    "28" => "MOSTLY_IQ1_S",
                    "29" => "MOSTLY_IQ4_NL",
                    "30" => "MOSTLY_IQ3_S",
                    "31" => "MOSTLY_IQ3_M",
                    "32" => "MOSTLY_IQ2_M",
                    "33" => "MOSTLY_IQ4_XS",
                    "34" => "MOSTLY_IQ1_M",
                    "40" | "41" => "TERNARY_1.58B",
                    _ => q.as_str(),
                };
                detected_quant = Some(human_quant.to_string());
            }
            res.quantization = detected_quant.clone();

            let quant_str = detected_quant.as_deref().unwrap_or("").to_uppercase();
            let arch_str = res.architecture.to_lowercase();
            let file_stem = weight_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

            let depth = if arch_str.contains("bitnet") || quant_str.contains("TERNARY") || file_stem.contains("1.58") || quant_str.contains("1.58") {
                "1.58-bit"
            } else if quant_str.contains("F32") {
                "32-bit"
            } else if quant_str.contains("F16") {
                "16-bit"
            } else if quant_str.contains("Q8") || quant_str.contains("MOSTLY_Q8") {
                "8-bit"
            } else if quant_str.contains("Q6") {
                "6-bit"
            } else if quant_str.contains("Q5") {
                "5-bit"
            } else if quant_str.contains("Q4") || quant_str.contains("MOSTLY_Q4") {
                "4-bit"
            } else if quant_str.contains("Q3") || quant_str.contains("IQ3") {
                "3-bit"
            } else if quant_str.contains("Q2") || quant_str.contains("IQ2") {
                "2-bit"
            } else if quant_str.contains("IQ1") {
                "1-bit"
            } else {
                "4-bit"
            };
            res.bit_depth = Some(depth.to_string());

            // 3. Compute parameter count
            let mut total_params: u64 = 0;
            if let Some(param_cnt_str) = metadata.get("general.parameter_count") {
                if let Ok(cnt) = param_cnt_str.parse::<u64>() {
                    total_params = cnt;
                }
            }
            if total_params == 0 {
                let has_embd = tensor_infos.keys().any(|k| k.contains("token_embd"));
                for (name, dims) in &tensor_infos {
                    // Skip tied lm_head output weight double counting
                    if has_embd && name.contains("output.weight") {
                        continue;
                    }
                    if !dims.is_empty() {
                        let count: u64 = dims.iter().map(|&d| d as u64).product();
                        total_params += count;
                    }
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
            res.has_vision_keys = metadata.keys().any(|k| {
                k.starts_with("clip.vision.") || k.starts_with("vision.") || k.starts_with("llava.")
            });
            res.has_vision_tensors = tensor_infos.iter().any(|(name, _)| {
                name.contains("visual") || name.contains("mm_projector") || name.contains("v.proj")
            });
            res.has_audio_keys = metadata.keys().any(|k| {
                k.starts_with("whisper.") || k.starts_with("audio.") || k.starts_with("bark.")
            });
            res.has_audio_tensors = tensor_infos.iter().any(|(name, _)| {
                name.contains("audio_encoder")
                    || name.contains("mel_filters")
                    || name.contains("speech")
            });
        }
    } else if format_type == "onnx" {
        // ONNX Prober Branch
        let file_stem = weight_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let parent_dir = weight_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Architecture Detection for ONNX
        if file_stem.contains("bge")
            || parent_dir.contains("bge")
            || file_stem.contains("bert")
            || parent_dir.contains("bert")
        {
            res.architecture = "embedding_encoder".to_string();
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
        } else if file_stem.contains("kokoro")
            || parent_dir.contains("kokoro")
            || file_stem.contains("tts")
            || parent_dir.contains("tts")
            || file_stem.contains("bark")
            || parent_dir.contains("bark")
        {
            res.architecture = "tts_engine".to_string();
            res.has_audio_keys = true;
            res.has_audio_tensors = true;
        } else {
            res.architecture = if !parent_dir.is_empty() {
                parent_dir
            } else {
                file_stem.clone()
            };
        }

        // Complete Multi-Bit Depth & Quantization Detection for ONNX / AWQ / GPTQ / BitNet
        if file_stem.contains("1.58bit")
            || file_stem.contains("ternary")
            || file_stem.contains("tq1")
            || file_stem.contains("tq2")
            || file_stem.contains("bitnet")
        {
            res.quantization = Some("1.58-bit Ternary".to_string());
            res.bit_depth = Some("1.58-bit".to_string());
        } else if file_stem.contains("q2")
            || file_stem.contains("2bit")
            || file_stem.contains("iq2")
        {
            res.quantization = Some("Q2_K".to_string());
            res.bit_depth = Some("2-bit".to_string());
        } else if file_stem.contains("q3")
            || file_stem.contains("3bit")
            || file_stem.contains("iq3")
        {
            res.quantization = Some("Q3_K".to_string());
            res.bit_depth = Some("3-bit".to_string());
        } else if file_stem.contains("q4")
            || file_stem.contains("4bit")
            || file_stem.contains("int4")
            || file_stem.contains("awq")
            || file_stem.contains("gptq")
        {
            res.quantization = Some("INT4 / Q4_K".to_string());
            res.bit_depth = Some("4-bit".to_string());
        } else if file_stem.contains("q5") || file_stem.contains("5bit") {
            res.quantization = Some("Q5_K".to_string());
            res.bit_depth = Some("5-bit".to_string());
        } else if file_stem.contains("q6") || file_stem.contains("6bit") {
            res.quantization = Some("Q6_K".to_string());
            res.bit_depth = Some("6-bit".to_string());
        } else if file_stem.contains("int8")
            || file_stem.contains("q8")
            || file_stem.contains("fp8")
        {
            res.quantization = Some("INT8".to_string());
            res.bit_depth = Some("8-bit".to_string());
        } else if file_stem.contains("fp16")
            || file_stem.contains("bfloat16")
            || file_stem.contains("bf16")
        {
            res.quantization = Some("FP16".to_string());
            res.bit_depth = Some("16-bit".to_string());
        } else {
            res.quantization = Some("FP32".to_string());
            res.bit_depth = Some("32-bit".to_string());
        }
    }

    res
}

/// No hardcoded reasoning tag guessing in prober. Metadata comes strictly from tokenizer files or remains None.
pub fn find_header_reasoning_tags(_tmpl: &str) -> (Option<String>, Option<String>) {
    (None, None)
}
