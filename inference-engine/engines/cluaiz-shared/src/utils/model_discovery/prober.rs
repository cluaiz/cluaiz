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
                if let Some(ctx) = metadata.get(&format!("{}.context_length", arch))
                    .or_else(|| metadata.get("whisper.max_audio_ctx"))
                    .or_else(|| metadata.get("whisper.context_length"))
                    .or_else(|| metadata.get("general.context_length"))
                {
                    if let Ok(val) = ctx.parse::<u64>() {
                        if val >= 1024 {
                            res.context_window = format!("{}K", val / 1024);
                        } else {
                            res.context_window = ctx.clone();
                        }
                    } else {
                        res.context_window = ctx.clone();
                    }
                } else if arch.contains("whisper") {
                    res.context_window = "30s (3000 frames)".to_string();
                }
            }

            if res.context_window == "Unknown" && (res.has_audio_keys || res.has_audio_tensors) {
                res.context_window = "30s (3000 frames)".to_string();
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
                for (_name, dims) in &tensor_infos {
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
                let kl = k.to_lowercase();
                kl.starts_with("clip.vision.")
                    || kl.starts_with("vision.")
                    || kl.starts_with("llava.")
                    || kl.starts_with("gemma4.")
                    || kl.starts_with("qwen2_vl.")
                    || kl.starts_with("qwen3_vl.")
                    || kl.starts_with("minicpm")
                    || kl.starts_with("mllama.")
                    || kl.starts_with("pixtral.")
                    || kl.starts_with("internvl.")
                    || kl.starts_with("internlm.")
                    || kl.starts_with("cogvlm.")
                    || kl.starts_with("moondream.")
                    || kl.starts_with("phi3v.")
                    || kl.starts_with("phi4v.")
                    || kl.starts_with("florence.")
                    || kl.starts_with("blip.")
                    || kl.starts_with("paligemma.")
                    || kl.starts_with("sam.")
                    || kl.starts_with("molmo.")
                    || kl.starts_with("deepseek_vl.")
            });
            res.has_vision_tensors = tensor_infos.iter().any(|(name, _)| {
                let n = name.to_lowercase();
                n.contains("visual")
                    || n.contains("mm_projector")
                    || n.contains("v.proj")
                    || n.contains("vision_encoder")
                    || n.contains("img_proj")
                    || n.contains("multi_modal")
                    || n.contains("image_newline")
                    || n.contains("img_attn")
                    || n.contains("patch_embed")
                    || n.contains("resampler")
                    || n.contains("vision_tower")
                    || n.contains("cross_attn")
            });
            if let Some(tmpl) = metadata.get("tokenizer.chat_template") {
                let tmpl_lower = tmpl.to_lowercase();
                if tmpl_lower.contains("<image>") || tmpl_lower.contains("<picture>") || tmpl_lower.contains("<|vision_start|>") || tmpl_lower.contains("image") {
                    res.has_vision_keys = true;
                }
            }

            res.has_audio_keys = metadata.keys().any(|k| {
                k.starts_with("whisper.") || k.starts_with("audio.") || k.starts_with("bark.") || k.starts_with("kokoro.")
            });
            res.has_audio_tensors = tensor_infos.iter().any(|(name, _)| {
                let n = name.to_lowercase();
                n.contains("audio_encoder")
                    || n.contains("mel_filters")
                    || n.contains("speech")
                    || n.contains("audio_proj")
            });
        }
    } else if format_type == "onnx" {
        // ONNX Prober Branch - Read binary Protobuf initializer headers directly
        let (_elements, param_str, ctx_window) = ONNXProber::probe(weight_path);
        if param_str != "Unknown" {
            res.parameters_str = param_str;
        }
        if let Some(ctx) = ctx_window {
            res.context_window = ctx;
        }

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
            res.context_window = "30s (3000 frames)".to_string();
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

    if let Some(parent) = weight_path.parent() {
        let manifest_path = parent.join("model_manifest.json");
        let config_path = parent.join("config.json");
        let target_json = if manifest_path.exists() {
            Some(manifest_path)
        } else if config_path.exists() {
            Some(config_path)
        } else {
            None
        };

        if let Some(jpath) = target_json {
            if let Ok(content) = std::fs::read_to_string(&jpath) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(tasks_arr) = json.get("supported_tasks").or_else(|| json.get("tasks")).and_then(|v| v.as_array()) {
                        for t in tasks_arr {
                            if let Some(ts) = t.as_str() {
                                if !res.explicit_tasks.contains(&ts.to_string()) {
                                    res.explicit_tasks.push(ts.to_string());
                                }
                            }
                        }
                    }
                    if let Some(task_str) = json.get("task").or_else(|| json.get("pipeline_tag")).and_then(|v| v.as_str()) {
                        if !res.explicit_tasks.contains(&task_str.to_string()) {
                            res.explicit_tasks.push(task_str.to_string());
                        }
                    }
                    if let Some(arch) = json.get("architectures").and_then(|v| v.as_array()).and_then(|arr| arr.first()).and_then(|v| v.as_str()) {
                        let arch_lower = arch.to_lowercase();
                        if arch_lower.contains("gemma4")
                            || arch_lower.contains("qwen2_vl")
                            || arch_lower.contains("qwen3_vl")
                            || arch_lower.contains("minicpm")
                            || arch_lower.contains("llava")
                            || arch_lower.contains("mllama")
                            || arch_lower.contains("pixtral")
                            || arch_lower.contains("internvl")
                            || arch_lower.contains("cogvlm")
                            || arch_lower.contains("moondream")
                            || arch_lower.contains("phi3v")
                            || arch_lower.contains("phi4v")
                            || arch_lower.contains("florence")
                            || arch_lower.contains("blip")
                            || arch_lower.contains("paligemma")
                            || arch_lower.contains("molmo")
                            || arch_lower.contains("deepseek_vl")
                        {
                            res.has_vision_keys = true;
                        }
                    }
                }
            }
        }
    }

    res
}

/// No hardcoded reasoning tag guessing in prober. Metadata comes strictly from tokenizer files or remains None.
pub fn find_header_reasoning_tags(_tmpl: &str) -> (Option<String>, Option<String>) {
    (None, None)
}

pub struct ONNXProber;

impl ONNXProber {
    pub fn probe(path: &Path) -> (u64, String, Option<String>) {
        let dir = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };

        let mut total_elements: u64 = 0;
        let mut ctx_window: Option<String> = None;

        if dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() && p.extension().map_or(false, |ext| ext == "onnx") {
                        if let Ok(mut file) = std::fs::File::open(&p) {
                            use std::io::Read;
                            let mut buffer = Vec::new();
                            if file.read_to_end(&mut buffer).is_ok() {
                                total_elements += Self::parse_protobuf_initializers(&buffer, &mut ctx_window);
                            }
                        }
                    }
                }
            }
        } else if path.is_file() {
            if let Ok(mut file) = std::fs::File::open(path) {
                use std::io::Read;
                let mut buffer = Vec::new();
                if file.read_to_end(&mut buffer).is_ok() {
                    total_elements = Self::parse_protobuf_initializers(&buffer, &mut ctx_window);
                }
            }
        }

        let param_str = if total_elements > 0 {
            format!("{:.2}B", total_elements as f64 / 1_000_000_000.0)
        } else {
            "Unknown".to_string()
        };

        (total_elements, param_str, ctx_window)
    }

    fn parse_protobuf_initializers(buf: &[u8], ctx_window: &mut Option<String>) -> u64 {
        let mut idx = 0;
        let mut total_elements: u64 = 0;
        let len = buf.len();

        while idx < len {
            let (tag_wire, new_idx) = match Self::read_varint(buf, idx) {
                Some(res) => res,
                None => break,
            };
            idx = new_idx;
            let field_num = tag_wire >> 3;
            let wire_type = tag_wire & 0x07;

            // ModelProto field 2 is GraphProto
            if field_num == 2 && wire_type == 2 {
                if let Some((length, new_idx)) = Self::read_varint(buf, idx) {
                    let end = (new_idx + length as usize).min(len);
                    total_elements += Self::parse_graph_proto(&buf[new_idx..end], ctx_window);
                    idx = end;
                    continue;
                }
            }
            idx = Self::skip_field(buf, idx, wire_type);
        }
        total_elements
    }

    fn parse_graph_proto(buf: &[u8], ctx_window: &mut Option<String>) -> u64 {
        let mut idx = 0;
        let mut total_elements: u64 = 0;
        let len = buf.len();

        while idx < len {
            let (tag_wire, new_idx) = match Self::read_varint(buf, idx) {
                Some(res) => res,
                None => break,
            };
            idx = new_idx;
            let field_num = tag_wire >> 3;
            let wire_type = tag_wire & 0x07;

            // GraphProto field 5 is TensorProto (initializer)
            if field_num == 5 && wire_type == 2 {
                if let Some((length, new_idx)) = Self::read_varint(buf, idx) {
                    let end = (new_idx + length as usize).min(len);
                    total_elements += Self::parse_tensor_proto(&buf[new_idx..end]);
                    idx = end;
                    continue;
                }
            } else if field_num == 11 && wire_type == 2 && ctx_window.is_none() {
                // GraphProto field 11 is input ValueInfoProto
                if let Some((length, new_idx)) = Self::read_varint(buf, idx) {
                    let end = (new_idx + length as usize).min(len);
                    if let Some(dim) = Self::parse_input_value_info(&buf[new_idx..end]) {
                        if dim == 77 {
                            *ctx_window = Some("77 Tokens (224x224 PX)".to_string());
                        } else if dim > 0 {
                            *ctx_window = Some(format!("{}", dim));
                        }
                    }
                    idx = end;
                    continue;
                }
            }
            idx = Self::skip_field(buf, idx, wire_type);
        }
        total_elements
    }

    fn parse_input_value_info(buf: &[u8]) -> Option<u64> {
        let mut idx = 0;
        let len = buf.len();

        while idx < len {
            let (tag_wire, new_idx) = match Self::read_varint(buf, idx) {
                Some(res) => res,
                None => break,
            };
            idx = new_idx;
            let field_num = tag_wire >> 3;
            let wire_type = tag_wire & 0x07;

            if field_num == 2 && wire_type == 2 {
                if let Some((length, new_idx)) = Self::read_varint(buf, idx) {
                    let end = (new_idx + length as usize).min(len);
                    return Self::parse_type_proto(&buf[new_idx..end]);
                }
            }
            idx = Self::skip_field(buf, idx, wire_type);
        }
        None
    }

    fn parse_type_proto(buf: &[u8]) -> Option<u64> {
        let mut idx = 0;
        let len = buf.len();

        while idx < len {
            let (tag_wire, new_idx) = match Self::read_varint(buf, idx) {
                Some(res) => res,
                None => break,
            };
            idx = new_idx;
            let field_num = tag_wire >> 3;
            let wire_type = tag_wire & 0x07;

            if field_num == 1 && wire_type == 2 {
                if let Some((length, new_idx)) = Self::read_varint(buf, idx) {
                    let end = (new_idx + length as usize).min(len);
                    return Self::parse_tensor_shape_proto(&buf[new_idx..end]);
                }
            }
            idx = Self::skip_field(buf, idx, wire_type);
        }
        None
    }

    fn parse_tensor_shape_proto(buf: &[u8]) -> Option<u64> {
        let mut idx = 0;
        let len = buf.len();

        while idx < len {
            let (tag_wire, new_idx) = match Self::read_varint(buf, idx) {
                Some(res) => res,
                None => break,
            };
            idx = new_idx;
            let field_num = tag_wire >> 3;
            let wire_type = tag_wire & 0x07;

            if field_num == 1 && wire_type == 2 {
                if let Some((length, new_idx)) = Self::read_varint(buf, idx) {
                    let end = (new_idx + length as usize).min(len);
                    let mut sub_idx = new_idx;
                    while sub_idx < end {
                        let (sub_tag, next_sub) = match Self::read_varint(buf, sub_idx) {
                            Some(r) => r,
                            None => break,
                        };
                        let fnum = sub_tag >> 3;
                        let wtype = sub_tag & 0x07;
                        if fnum == 1 && wtype == 2 {
                            if let Some((dim_len, d_idx)) = Self::read_varint(buf, next_sub) {
                                let d_end = (d_idx + dim_len as usize).min(end);
                                let mut d_sub = d_idx;
                                while d_sub < d_end {
                                    let (d_tag, d_next) = match Self::read_varint(buf, d_sub) {
                                        Some(r) => r,
                                        None => break,
                                    };
                                    if (d_tag >> 3) == 1 && (d_tag & 0x07) == 0 {
                                        if let Some((val, _)) = Self::read_varint(buf, d_next) {
                                            if val > 1 && val <= 32768 {
                                                return Some(val);
                                            }
                                        }
                                    }
                                    d_sub = Self::skip_field(buf, d_next, d_tag & 0x07);
                                }
                            }
                        }
                        sub_idx = Self::skip_field(buf, next_sub, wtype);
                    }
                }
            }
            idx = Self::skip_field(buf, idx, wire_type);
        }
        None
    }

    fn parse_tensor_proto(buf: &[u8]) -> u64 {
        let mut idx = 0;
        let mut dims: Vec<u64> = Vec::new();
        let len = buf.len();

        while idx < len {
            let (tag_wire, new_idx) = match Self::read_varint(buf, idx) {
                Some(res) => res,
                None => break,
            };
            idx = new_idx;
            let field_num = tag_wire >> 3;
            let wire_type = tag_wire & 0x07;

            // TensorProto field 7 is dims (repeated int64)
            if field_num == 7 {
                if wire_type == 0 {
                    if let Some((dim_val, new_idx)) = Self::read_varint(buf, idx) {
                        dims.push(dim_val);
                        idx = new_idx;
                        continue;
                    }
                } else if wire_type == 2 {
                    if let Some((length, new_idx)) = Self::read_varint(buf, idx) {
                        let end = (new_idx + length as usize).min(len);
                        let mut sub_idx = new_idx;
                        while sub_idx < end {
                            if let Some((d, next_sub)) = Self::read_varint(buf, sub_idx) {
                                dims.push(d);
                                sub_idx = next_sub;
                            } else {
                                break;
                            }
                        }
                        idx = end;
                        continue;
                    }
                }
            }
            idx = Self::skip_field(buf, idx, wire_type);
        }

        if !dims.is_empty() {
            dims.iter().product()
        } else {
            0
        }
    }

    fn read_varint(buf: &[u8], mut idx: usize) -> Option<(u64, usize)> {
        let mut val: u64 = 0;
        let mut shift = 0;
        while idx < buf.len() {
            let byte = buf[idx];
            idx += 1;
            val |= ((byte & 0x7f) as u64) << shift;
            if (byte & 0x80) == 0 {
                return Some((val, idx));
            }
            shift += 7;
            if shift >= 64 {
                break;
            }
        }
        None
    }

    fn skip_field(buf: &[u8], idx: usize, wire_type: u64) -> usize {
        match wire_type {
            0 => {
                let mut i = idx;
                while i < buf.len() && (buf[i] & 0x80) != 0 {
                    i += 1;
                }
                if i < buf.len() { i + 1 } else { buf.len() }
            }
            1 => (idx + 8).min(buf.len()),
            2 => {
                if let Some((len, new_idx)) = Self::read_varint(buf, idx) {
                    (new_idx + len as usize).min(buf.len())
                } else {
                    buf.len()
                }
            }
            5 => (idx + 4).min(buf.len()),
            _ => buf.len(),
        }
    }
}
