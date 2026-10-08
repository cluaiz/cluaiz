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
        "Q4_K_S", "Q4_K_M", "Q4_K_L", "Q4_K", "Q4_0", "Q4_1",
        "Q5_K_S", "Q5_K_M", "Q5_K_L", "Q5_K", "Q5_0", "Q5_1",
        "Q6_K", "Q6_0",
        "Q8_0", "Q8_1", "Q8_K",
        "BF16", "FP16", "F16", "FP32", "F32",
        "FP8_E4M3", "FP8_E5M2", "FP8", "MXFP8",
        "FP6", "MXFP6",
        "FP4", "MXFP4",
        "INT8", "UINT8", "INT4", "UINT4", "BNB4", "INT2", "UINT2",
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

    /// Checks if a file path is a multi-part shard and returns the common base name prefix.
    pub fn extract_shard_base(path_str: &str) -> Option<String> {
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
        
        let parts: Vec<&str> = clean.split(&['-', '_', '.'][..]).collect();

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

        // Strip multi-part sharding suffixes using SSOT patterns
        for pattern in Self::SHARD_PATTERNS {
            if let Some(idx) = clean_name.to_lowercase().find(pattern) {
                clean_name = clean_name[..idx].to_string();
                break;
            }
        }

        let upper = clean_name.to_uppercase();

        // 3. Priority candidates matching
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
        } else if q.contains("q2") || q.contains("iq2") || q.contains("iq1") || q.contains("1.58") || q.contains("1bit") {
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
