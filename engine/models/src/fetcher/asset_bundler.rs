//! ═══════════════════════════════════════════════════════════════════════
//!   Fetcher: Universal Asset Bundler & Companion File Resolver
//! ═══════════════════════════════════════════════════════════════════════

use std::collections::HashSet;
use std::path::Path;
use crate::models::taxonomy::quantization::UniversalQuantization;
use crate::models::taxonomy::tts_families::TtsTaxonomy;

pub struct AssetBundler;

impl AssetBundler {
    /// Returns supplementary metadata and configuration file names to auto-fetch for a model repository.
    pub fn supplementary_metadata_files() -> &'static [&'static str] {
        &[
            "config.json",
            "configuration.json",
            "generation_config.json",
            "tokenizer.json",
            "tokenizer_config.json",
            "special_tokens_map.json",
            "vocab.json",
            "tokens.txt",
            "tokens.json",
            "lexicon.txt",
            "voices.json",
            "cosyvoice.yaml",
            "chat_template.json",
            "processor_config.json",
            "preprocessor_config.json",
            "quantize_config.json",
            "model.onnx.json",
            "model.onnx.yaml",
            "speaker_embeddings.bin",
            "cluaiz-engine.ready",
        ]
    }

    /// Evaluates candidate model filenames for primary graph priority scoring.
    pub fn score_model_file_priority(category: &str, filename: &str) -> usize {
        if category == "tts" || category == "audio" {
            TtsTaxonomy::score_tts_file(filename)
        } else if category == "stt" {
            crate::models::taxonomy::stt_families::SttTaxonomy::score_stt_file_priority(filename)
        } else {
            1
        }
    }

    /// Returns true if a filename is a PyTorch / raw weight file that should be excluded.
    pub fn is_pytorch_weights(filename: &str) -> bool {
        let path = Path::new(filename);
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext == "pt" || ext == "pth" || ext == "safetensors" || ext == "ckpt" {
            return true;
        }
        if ext == "bin"
            && !filename.to_lowercase().contains("speaker")
            && !filename.to_lowercase().contains("voices")
        {
            return true;
        }
        false
    }

    /// Returns true if the GGUF is ANY kind of helper file (MTP, mmproj, projector, adapter).
    pub fn is_helper_gguf(filename: &str) -> bool {
        Self::is_mtp_gguf(filename)
            || Self::is_mmproj_gguf(filename)
            || Self::is_projector_gguf(filename)
            || Self::is_adapter_gguf(filename)
    }

    pub fn is_mtp_gguf(filename: &str) -> bool {
        let b = Self::basename_of(filename);
        b.starts_with("mtp-")
            || b.contains("-mtp-")
            || b.contains("_mtp-")
            || filename.to_lowercase().contains("/mtp/")
    }

    pub fn is_mmproj_gguf(filename: &str) -> bool {
        let b = Self::basename_of(filename);
        b.starts_with("mmproj-") || b.contains("-mmproj-") || b.contains("_mmproj-")
    }

    pub fn is_projector_gguf(filename: &str) -> bool {
        let b = Self::basename_of(filename);
        b.starts_with("projector-") || b.contains("-projector-") || b.contains("_projector-")
    }

    pub fn is_adapter_gguf(filename: &str) -> bool {
        let b = Self::basename_of(filename);
        b.starts_with("adapter-") || b.contains("-adapter-") || b.contains("_adapter-")
    }

    pub fn basename_of(path: &str) -> String {
        Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(path)
            .to_lowercase()
    }

    /// From a list of mmproj file paths, selects the single best one to download (F16 → BF16 → FP16 → F32).
    pub fn select_best_mmproj<'a>(mmproj_paths: &[&'a str]) -> Option<&'a str> {
        if mmproj_paths.is_empty() {
            return None;
        }
        let preferred = ["f16", "bf16", "fp16", "f32", "fp32"];
        for pref in &preferred {
            for &path in mmproj_paths {
                if Self::basename_of(path).contains(pref) {
                    return Some(path);
                }
            }
        }
        Some(mmproj_paths[0])
    }

    /// From a list of MTP file paths and a main model quant tag, selects the single best MTP to download.
    pub fn select_best_mtp<'a>(mtp_paths: &[&'a str], main_quant: &str) -> Option<&'a str> {
        if mtp_paths.is_empty() {
            return None;
        }
        let main_tier = UniversalQuantization::quant_tier_of(main_quant) as i32;
        mtp_paths
            .iter()
            .min_by_key(|&&path| {
                let mtp_quant = Self::basename_of(path);
                let mtp_tier = UniversalQuantization::quant_tier_of(&mtp_quant) as i32;
                (main_tier - mtp_tier).unsigned_abs() as usize
            })
            .copied()
    }

    /// Evaluates if metadata from a subfolder is compatible with the primary model variant.
    pub fn is_compatible_subfolder_metadata(meta_path: &str, primary_path: &str) -> bool {
        let lower = meta_path.to_lowercase();
        let is_asset = lower.ends_with(".json") 
            || lower.ends_with(".txt") 
            || lower.ends_with(".yaml") 
            || lower.ends_with(".yml") 
            || lower.ends_with(".onnx")
            || lower.ends_with(".onnx.data")
            || lower.ends_with(".bin")
            || lower.contains("vocab") 
            || lower.contains("merges") 
            || lower.contains("token")
            || lower.contains("voices");
            
        if !is_asset {
            return false;
        }

        let meta_dir = Path::new(meta_path).parent().and_then(|p| p.to_str()).unwrap_or("");
        if meta_dir.is_empty() {
            return true; // Root-level files are shared
        }

        let meta_dir_lower = meta_dir.to_lowercase();
        let primary_lower = primary_path.to_lowercase();

        // Isolate quant-specific folders (e.g. Q4 config shouldn't attach to FP16 model)
        if (meta_dir_lower.contains("q4") || meta_dir_lower.contains("int8") || meta_dir_lower.contains("fp16") || meta_dir_lower.contains("quant")) 
            && !primary_lower.contains(&meta_dir_lower) 
            && !meta_dir_lower.contains("voices")
        {
            return false;
        }

        true
    }

    /// Drops root metadata files when a specific subfolder counterpart exists in the bundle.
    pub fn filter_duplicate_metadata_files(bundle_files: &mut Vec<String>) {
        let mut subfolder_basenames = HashSet::new();
        for f in bundle_files.iter() {
            if let Some(parent) = Path::new(f).parent() {
                if !parent.as_os_str().is_empty() {
                    if let Some(basename) = Path::new(f).file_name().and_then(|s| s.to_str()) {
                        subfolder_basenames.insert(basename.to_lowercase());
                    }
                }
            }
        }

        bundle_files.retain(|f| {
            let path = Path::new(f);
            let parent = path.parent().and_then(|p| p.to_str()).unwrap_or("");
            if parent.is_empty() {
                if let Some(basename) = path.file_name().and_then(|s| s.to_str()) {
                    if subfolder_basenames.contains(&basename.to_lowercase()) {
                        return false;
                    }
                }
            }
            true
        });
    }

    /// Resolves the canonical sovereign model ID.
    pub fn resolve_sovereign_id(repo_id: &str, name: &str, _filename: &str, quant: &str) -> String {
        let repo_id = if repo_id.contains(':') {
            repo_id.split(':').next().unwrap_or(repo_id)
        } else {
            repo_id
        };
        let repo_basename = repo_id
            .split('/')
            .next_back()
            .unwrap_or(repo_id)
            .to_lowercase();
        let clean_file_stem = name.to_lowercase();

        let clean_repo = repo_basename
            .replace("-gguf", "")
            .replace("-onnx", "")
            .replace("_gguf", "")
            .replace("_onnx", "");

        if clean_file_stem == "model"
            || clean_file_stem == "model_q4"
            || clean_file_stem == "pytorch_model"
            || clean_file_stem.is_empty()
        {
            repo_basename
        } else if clean_file_stem == repo_basename {
            repo_basename
        } else if clean_file_stem.contains(&clean_repo) {
            if quant != "unknown" && !quant.is_empty() {
                let quant_upper = quant.to_uppercase();
                let repo_upper = repo_basename.to_uppercase();
                if repo_upper.contains(&quant_upper) {
                    repo_basename
                } else {
                    format!("{}-{}", repo_basename, quant_upper)
                }
            } else {
                repo_basename
            }
        } else {
            format!("{}-{}", repo_basename, clean_file_stem)
        }
    }
}
