//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Universal Quantization & Precision Engine (Single Source of Truth)
//! ═══════════════════════════════════════════════════════════════════════

use std::path::Path;

pub struct UniversalQuantization;

impl UniversalQuantization {
    /// Canonical multi-part weight shard patterns
    pub const SHARD_PATTERNS: &'static [&'static str] = &[
        "-00001-of-",
        "_00001-of-",
        "-00001.",
        "_00001.",
        ".part1of",
        ".part01.",
        "split-00001-of-",
    ];

    /// Canonical ONNX companion data extensions and suffixes
    pub const ONNX_COMPANION_SUFFIXES: &'static [&'static str] = &[
        ".onnx.data",
        ".onnx_data",
        ".data",
    ];

    /// Canonical list of known quantization identifiers (sorted longest/most specific first)
    pub const QUANT_CANDIDATES: &'static [&'static str] = &[
        "IQ1_XXS", "IQ1_S", "IQ1_M",
        "IQ2_XXS", "IQ2_XS", "IQ2_S", "IQ2_M",
        "IQ3_XXS", "IQ3_XS", "IQ3_S", "IQ3_M",
        "IQ4_XS", "IQ4_NL",
        "UD-Q4_K_M", "UD-Q4_K_S", "UD-Q5_K_M", "UD-Q8_0",
        "Q2_K_S", "Q2_K",
        "Q3_K_S", "Q3_K_M", "Q3_K_L", "Q3_K",
        "Q4_K_S", "Q4_K_M", "Q4_K_L", "Q4_K", "Q4_0", "Q4_1", "Q4F16", "Q4",
        "Q5_K_S", "Q5_K_M", "Q5_K_L", "Q5_K", "Q5_0", "Q5_1",
        "Q6_K", "Q6_0",
        "Q8_0", "Q8_1", "Q8_K", "Q8F16", "Q8",
        "BF16", "FP16", "F16", "FP32", "F32",
        "FP8_E4M3", "FP8_E5M2", "FP8", "MXFP8",
        "FP6", "MXFP6",
        "FP4", "MXFP4",
        // A name containing another goes first ("UINT8" contains "INT8")
        "UINT8F16", "UINT8", "INT8", "UINT4", "INT4", "BNB4", "UINT2", "INT2",
        "QUANTIZED",
        "1.58BIT", "BITNET", "1BIT", "1-BIT",
    ];

    /// Strips quantization suffix and extension to return the normalized model base stem.
    pub fn strip_quant(p: &str) -> String {
        let stem = Path::new(p)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(p)
            .to_lowercase();
        let suffixes = [
            "_q4", "_q4f16", "_q8f16", "_fp16", "_int8", "_int4",
            "_uint8", "_uint8f16", "_quantized", "_bnb4", "_q8", "_q2",
            ".fp32", ".int8", "-q4", "-q8", "-fp16",
        ];
        let mut clean = stem;
        for s in &suffixes {
            if clean.ends_with(s) {
                clean = clean[..clean.len() - s.len()].to_string();
                break;
            }
        }
        clean
    }

    /// A `gguf-split` file name, `<prefix>-NNNNN-of-NNNNN.gguf`, as llama.cpp reads it
    /// (`common/download.cpp`, `get_gguf_split_info`): the prefix with its folder, the index and the count.
    /// Every shard of a set gives the same prefix and count, whatever its index.
    pub fn gguf_split(path_str: &str) -> Option<(String, u32, u32)> {
        let lower = path_str.to_lowercase();
        let stem = lower.strip_suffix(".gguf")?;
        let (rest, count) = stem.rsplit_once("-of-")?;
        let (prefix, index) = rest.rsplit_once('-')?;
        let digits = |s: &str| s.len() == 5 && s.bytes().all(|b| b.is_ascii_digit());
        if prefix.is_empty() || !digits(index) || !digits(count) {
            return None;
        }
        Some((prefix.to_string(), index.parse().ok()?, count.parse().ok()?))
    }

    /// Checks if a file path is a multi-part shard and returns the common base name prefix.
    /// A GGUF split set gives one base for all its shards (`prefix` and the shard count).
    pub fn extract_shard_base(path_str: &str) -> Option<String> {
        if let Some((prefix, _, count)) = Self::gguf_split(path_str) {
            return (count > 1).then(|| format!("{prefix}-of-{count:05}"));
        }
        let lower = path_str.to_lowercase();
        for pat in Self::SHARD_PATTERNS {
            if let Some(pos) = lower.find(pat) {
                return Some(lower[..pos].to_string());
            }
        }
        None
    }

    /// Strips `.onnx`, `.onnx.data`, `.onnx_data`, or `.data` to yield the canonical ONNX model stem.
    pub fn strip_onnx_companion_suffix(path_str: &str) -> String {
        let p_lower = path_str.to_lowercase();
        let suffixes = [".onnx.data", ".onnx_data", ".onnx", ".data"];
        for suffix in &suffixes {
            if p_lower.ends_with(suffix) {
                return path_str[..path_str.len() - suffix.len()].to_string();
            }
        }
        path_str.to_string()
    }

    /// Returns true if a path is an ONNX external data companion file.
    pub fn is_onnx_companion_file(path_str: &str) -> bool {
        let lower = path_str.to_lowercase();
        Self::ONNX_COMPANION_SUFFIXES.iter().any(|s| lower.ends_with(s))
    }

    /// Checks if an arbitrary token is a quantization or format indicator.
    pub fn is_quant_token(token: &str) -> bool {
        let upper = token.to_uppercase();
        if upper.is_empty() {
            return false;
        }

        // Direct candidate match
        if Self::QUANT_CANDIDATES.iter().any(|&c| c == upper || upper.contains(c)) {
            return true;
        }

        // Dynamic prefix match (e.g. Q4, Q8, INT8, FP16, etc.)
        if upper.starts_with('Q') && upper.chars().nth(1).map_or(false, |c| c.is_ascii_digit()) {
            return true;
        }

        matches!(upper.as_str(), "GGUF" | "ONNX" | "AWQ" | "GPTQ" | "DEFAULT" | "UNKNOWN")
    }

    /// Extracts parameter count string (e.g. 7B, 8x7B, 70B, 1.5B, 0.5B) by filtering out format & quant tokens.
    pub fn extract_parameters_from_name(name_str: &str, quant_tag: &str) -> String {
        let clean = name_str.to_lowercase()
            .replace(".gguf", "")
            .replace(".onnx", "")
            .replace(".bin", "");
        
        // Not split on '.': sizes such as 0.6B and 1.5B carry one
        let parts: Vec<&str> = clean.split(&['-', '_'][..]).collect();

        for part in parts {
            let part_upper = part.to_uppercase();
            if (!quant_tag.is_empty() && quant_tag.to_uppercase().contains(&part_upper))
                || Self::is_quant_token(part)
            {
                continue;
            }

            if part.ends_with('b') && (part.chars().next().unwrap_or('a').is_ascii_digit() || part.contains('x')) {
                return part_upper;
            }
        }
        "Unknown".to_string()
    }

    /// Authoritatively extracts the normalized quantization tag from a file path or name.
    pub fn extract_quant_tag(path_str: &str) -> String {
        let path_obj = Path::new(path_str);

        // 1. Check parent folder name if inside a named subfolder (e.g., Q4_K_M/, BF16/)
        if let Some(parent) = path_obj.parent().and_then(|p| p.to_str()) {
            if !parent.is_empty() && parent != "." {
                let p_lower = parent.to_lowercase();
                if p_lower != "onnx" && p_lower != "models" && p_lower != "weights" && p_lower != "chat" && p_lower != "embedding" && p_lower != "vision" && p_lower != "audio" {
                    if p_lower.contains('q')
                        || p_lower.contains("bf16")
                        || p_lower.contains("fp16")
                        || p_lower.contains("int")
                        || p_lower.contains("ud")
                    {
                        return parent.to_string().to_uppercase();
                    }
                }
            }
        }

        // 2. Clean filename (strip extensions and multi-part shard suffixes)
        let file_name = path_obj
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path_str);
        
        let mut clean_name = file_name.to_string();
        for ext in &[".gguf", ".onnx", ".onnx_data", ".onnx.data", ".bin"] {
            if clean_name.to_lowercase().ends_with(ext) {
                clean_name = clean_name[..clean_name.len() - ext.len()].to_string();
            }
        }

        // Strip multi-part sharding suffixes: any gguf-split index, then the SSOT patterns
        if let Some((_, _, count)) = Self::gguf_split(file_name) {
            let marker = format!("-of-{count:05}");
            if let Some(end) = clean_name.to_lowercase().rfind(&marker) {
                clean_name.truncate(end.saturating_sub(6));
            }
        }
        for pattern in Self::SHARD_PATTERNS {
            if let Some(idx) = clean_name.to_lowercase().find(pattern) {
                clean_name = clean_name[..idx].to_string();
                break;
            }
        }

        let upper = clean_name.to_uppercase();

        // 3. A GGUF's tag is its last name part, as llama.cpp reads it (`[-.]([A-Z0-9_]+)$`), when that part
        // is a quantization: `UD-Q4_K_XL`, `Q2_K_L`, `TQ2_0`, `PQ2_0`, `Q1_0` keep their full names
        if path_str.to_lowercase().ends_with(".gguf") {
            if let Some(tag) = Self::gguf_name_tag(&upper) {
                return tag;
            }
        }

        // 4. Priority candidates matching
        for &cand in Self::QUANT_CANDIDATES {
            if upper.contains(cand) {
                return cand.to_string();
            }
        }

        // Fallback based on extension
        if path_str.to_lowercase().ends_with(".onnx") {
            "ONNX".to_string()
        } else {
            "GGUF".to_string()
        }
    }

    /// The quantization a GGUF name ends with (upper case, extension and split suffix removed), or None
    /// when the last name part is not one: a known type prefix and its bit count, then `_` parts only
    /// (`Q4_K_XL`, `IQ2_XXS`, `TQ1_0`, `BF16`, `MXFP4`), with `UD-` kept when the name has it.
    pub(crate) fn gguf_name_tag(upper: &str) -> Option<String> {
        let last = upper.rsplit(['-', '.']).next()?;
        let kind = ["MXFP", "NVFP", "PTQ", "IQ", "TQ", "PQ", "BF", "FP", "Q", "F"]
            .iter()
            .find(|kind| last.starts_with(*kind))?;
        let rest = &last[kind.len()..];
        if !rest.starts_with(|c: char| c.is_ascii_digit())
            || !rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return None;
        }
        let ud = upper.len() > last.len() + 3 && upper[..upper.len() - last.len()].ends_with("-UD-");
        Some(if ud { format!("UD-{last}") } else { last.to_string() })
    }

    /// Authoritatively estimates numeric bit-depth (e.g. 1.0, 1.58, 4.0, 8.0, 16.0) from a quantization tag.
    pub fn estimate_bit_depth(quant_tag: &str) -> f64 {
        let upper = quant_tag.to_uppercase();

        if upper.contains("1.58") || upper.contains("BITNET") {
            1.58
        } else if upper.contains("1BIT") || upper.contains("1-BIT") || upper.contains("INT1") || upper.contains("UINT1") {
            1.0
        } else if upper.contains("IQ1_S") {
            1.56
        } else if upper.contains("IQ1_M") || upper.contains("IQ1") {
            1.75
        } else if upper.contains("IQ2_XXS") {
            2.06
        } else if upper.contains("IQ2_XS") {
            2.31
        } else if upper.contains("IQ2_S") {
            2.50
        } else if upper.contains("IQ2_M") || upper.contains("Q2_K") || upper.contains("Q2") || upper.contains("INT2") {
            2.56
        } else if upper.contains("IQ3_XXS") {
            3.06
        } else if upper.contains("IQ3") || upper.contains("Q3") || upper.contains("INT3") {
            3.50
        } else if upper.contains("IQ4_XS") {
            4.25
        } else if upper.contains("IQ4_NL") {
            4.50
        } else if upper.contains("Q4") || upper.contains("INT4") || upper.contains("UINT4") || upper.contains("BNB4") || upper.contains("FP4") || upper.contains("MXFP4") {
            4.0
        } else if upper.contains("Q5") || upper.contains("INT5") {
            5.0
        } else if upper.contains("Q6") || upper.contains("FP6") || upper.contains("MXFP6") {
            6.0
        } else if upper.contains("Q8") || upper.contains("INT8") || upper.contains("UINT8") || upper.contains("FP8") || upper.contains("MXFP8") {
            8.0
        } else if upper.contains("BF16") || upper.contains("FP16") || upper.contains("F16") {
            16.0
        } else if upper.contains("FP32") || upper.contains("F32") {
            32.0
        } else {
            4.0
        }
    }

    /// Returns an integer tier (0-8) for quantization quality ranking.
    pub fn quant_tier_of(quant: &str) -> usize {
        let q = quant.to_lowercase();
        if q.contains("f32") || q.contains("fp32") {
            8
        } else if q.contains("f16") || q.contains("bf16") || q.contains("fp16") {
            7
        } else if q.contains("q8") || q.contains("int8") || q.contains("fp8") {
            6
        } else if q.contains("q6") || q.contains("fp6") {
            5
        } else if q.contains("q5") {
            4
        } else if q.contains("q4") || q.contains("iq4") || q.contains("ud-q4") || q.contains("int4") {
            3
        } else if q.contains("q3") || q.contains("iq3") {
            2
        } else if q.contains("q2") || q.contains("iq2") || q.contains("iq1") || q.contains("q1_") || q.contains("1.58") || q.contains("1bit") {
            1
        } else {
            0
        }
    }

    /// Resolves canonical format string ("gguf" or "onnx") from a file path.
    pub fn resolve_format_type(path: &str) -> &'static str {
        if path.to_lowercase().ends_with(".onnx") {
            "onnx"
        } else {
            "gguf"
        }
    }

    /// Returns true if a path points to an executable weight binary (.gguf or .onnx).
    pub fn is_executable_weight_file(path: &str) -> bool {
        let lower = path.to_lowercase();
        lower.ends_with(".gguf") || lower.ends_with(".onnx")
    }
}

/// Tensor types the bundled llama.cpp can read, lower case (`q4_k`, `tq1_0`): set once at start by the
/// program that links llama.cpp (`llama_engine::tensor_type_names`). Unset, nothing is judged.
static LOADABLE_TYPES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// Records which tensor types this build can load (first call wins)
pub fn set_loadable_tensor_types(names: Vec<String>) {
    let _ = LOADABLE_TYPES.set(names);
}

/// Whether a GGUF variant named with quantization `tag` can be loaded by this build: `Some(false)` only
/// when the tag is a quantization name (`PTQ1_0`, `Q4_K_M`) and no tensor type of its family and bit
/// count is in the list; `None` when the list is not set or the tag is not a quantization name
/// (nothing is hidden then). The family and bits (`q4`, `iq3`, `tq1`, `mxfp4`) are compared, not the
/// whole name: llama.cpp's mixes are named after their main type and a size (`Q4_K_M` holds `q4_K`
/// tensors, `IQ3_M` holds `iq3_s`).
pub fn tag_loadable(tag: &str) -> Option<bool> {
    tag_loadable_in(LOADABLE_TYPES.get()?, tag)
}

fn tag_loadable_in(types: &[String], tag: &str) -> Option<bool> {
    let upper = tag.to_uppercase();
    let name = upper.strip_prefix("UD-").unwrap_or(&upper);
    UniversalQuantization::gguf_name_tag(name)?;
    let lower = name.to_lowercase();
    let lower = lower.strip_prefix("fp").map_or(lower.clone(), |rest| format!("f{rest}"));
    let stem = lower.split('_').next().unwrap_or(&lower);
    Some(types.iter().any(|t| t == stem || t.starts_with(&format!("{stem}_"))))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_quantization_is_loadable_only_when_its_tensor_type_is_known() {
        let types: Vec<String> = ["f32", "f16", "bf16", "q1_0", "q2_0", "q4_0", "q8_0", "mxfp4", "q2_k", "q4_k", "q6_k", "iq2_xxs", "iq3_s", "iq4_xs", "iq1_m", "tq1_0", "tq2_0"]
            .iter()
            .map(|t| t.to_string())
            .collect();
        let loadable = |tag: &str| super::tag_loadable_in(&types, tag);
        for tag in ["Q4_K_M", "UD-Q4_K_XL", "Q2_K_L", "IQ4_XS", "IQ1_M", "IQ2_XXS", "IQ3_M", "IQ3_XS", "TQ1_0", "Q1_0", "Q2_0", "BF16", "F16", "FP16", "MXFP4", "Q8_0"] {
            assert_eq!(loadable(tag), Some(true), "{tag}");
        }
        for tag in ["PTQ1_0", "PQ2_0", "Q5_K_M"] {
            assert_eq!(loadable(tag), Some(false), "{tag}");
        }
        for tag in ["GGUF", "DEFAULT", "CUSTOM", "ONNX"] {
            assert_eq!(loadable(tag), None, "{tag}: not a quantization name, nothing is judged");
        }
    }

    use super::UniversalQuantization;

    #[test]
    fn every_shard_of_a_split_set_has_the_same_base() {
        let q = UniversalQuantization::extract_shard_base;
        let first = q("BF16/Qwen3.8-Flash-Next-BF16-00001-of-00008.gguf");
        assert!(first.is_some());
        assert_eq!(q("BF16/Qwen3.8-Flash-Next-BF16-00002-of-00008.gguf"), first);
        assert_eq!(q("BF16/Qwen3.8-Flash-Next-BF16-00008-of-00008.gguf"), first);
        assert_ne!(q("Q8_0/Qwen3.8-Flash-Next-Q8_0-00002-of-00006.gguf"), first);
        assert_eq!(q("Qwen3-8B-Q4_K_M.gguf"), None);
        assert_eq!(q("Model-Q4_K_M-00001-of-00001.gguf"), None);
        assert_eq!(UniversalQuantization::gguf_split("a/Model-Q8_0-00003-of-00006.gguf"), Some(("a/model-q8_0".into(), 3, 6)));
    }

    #[test]
    fn a_gguf_tag_is_its_last_name_part() {
        let t = UniversalQuantization::extract_quant_tag;
        assert_eq!(t("Qwen3.8-Flash-Next-UD-Q4_K_XL-00001-of-00004.gguf"), "UD-Q4_K_XL");
        assert_eq!(t("Kimi-K3-UD-TQ2_0-00001-of-00013.gguf"), "UD-TQ2_0");
        assert_eq!(t("Kimi-K3-UD-Q1_0-00001-of-00011.gguf"), "UD-Q1_0");
        assert_eq!(t("DeepSeek-R1-Distill-Qwen-32B-Q2_K_L.gguf"), "Q2_K_L");
        assert_eq!(t("Ternary-Bonsai-2-4B-PQ2_0.gguf"), "PQ2_0");
        assert_eq!(t("Ternary-Bonsai-2-4B-Q2_0.gguf"), "Q2_0");
        assert_eq!(t("Qwen3.5-4B-IQ4_NL.gguf"), "IQ4_NL");
        assert_eq!(t("Llama-3.2-3B-Instruct-Q4_K_M.gguf"), "Q4_K_M");
        assert_eq!(t("gpt-oss-120b-MXFP4.gguf"), "MXFP4");
        assert_eq!(t("Qwen3.5-4B-BF16.gguf"), "BF16");
        assert_eq!(t("Qwen3-8B-UD-Q4_K_M.gguf"), "UD-Q4_K_M");
        // No quantization at the end: the known names inside, as before
        assert_eq!(t("model-q4f16.onnx"), "Q4F16");
        assert_eq!(t("Bonsai-8B.gguf"), "GGUF");
        assert_eq!(UniversalQuantization::quant_tier_of("UD-Q1_0"), 1);
    }

    #[test]
    fn a_shard_keeps_its_quant_tag() {
        let t = UniversalQuantization::extract_quant_tag;
        assert_eq!(t("Q8_0/Qwen3.8-Flash-Next-Q8_0-00003-of-00006.gguf"), t("Qwen3.8-Flash-Next-Q8_0-00001-of-00006.gguf"));
        assert_eq!(t("Qwen3.8-Flash-Next-BF16-00005-of-00008.gguf"), "BF16");
    }

    #[test]
    fn parameters_keep_the_decimal_point() {
        let p = |name: &str| UniversalQuantization::extract_parameters_from_name(name, "Q4_K_M");
        assert_eq!(p("Qwen3-0.6B-Q4_K_M.gguf"), "0.6B");
        assert_eq!(p("Qwen2.5-1.5B-Instruct-Q4_K_M.gguf"), "1.5B");
        assert_eq!(p("Llama-3.2-3B-Instruct-Q4_K_M.gguf"), "3B");
        assert_eq!(p("Mixtral-8x7B-v0.1-Q4_K_M.gguf"), "8X7B");
        assert_eq!(p("Qwen3-4B-Q4_K_M.gguf"), "4B");
    }
}
