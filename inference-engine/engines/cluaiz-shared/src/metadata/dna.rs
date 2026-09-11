use crate::backend::signature::{BackendType, KernelSignature};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

// ─── Structural DNA Synchronization (The Root Genome) ──────────────────────
#[derive(Debug, Clone, Deserialize, Serialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct StructuralDNA {
    pub model_identity: String,
    pub layer_count: Option<usize>,
    pub attention_head_count: Option<usize>,
    pub attention_head_count_kv: Option<usize>,
    pub attention_head_dim: Option<usize>,
    pub hidden_size: Option<usize>,
    pub intermediate_size: Option<usize>,
    pub attention_dimensionality_truth: Option<usize>,
    pub signature: KernelSignature,
    pub preferred_runtime: Option<BackendType>,
    pub heterogeneous_map: Option<HashMap<String, usize>>,
    pub max_context_length: Option<usize>,
    pub eos_token: Option<String>,
    pub chat_template: Option<String>,
    pub stop_sequences: Vec<String>,
    pub inference_params: HashMap<String, String>,
    pub dynamic_attributes: HashMap<String, String>,
    // Hardware Context
    pub vram_headroom_gb: f32,
    pub ram_headroom_gb: f32,
    pub requires_gpu: bool,
    pub weights_size_gb: f32,

    #[serde(default)]
    pub weights_already_loaded: bool,

    // 🎯 Active Inference State
    #[serde(default)]
    pub guidance_bias: Option<HashMap<i32, f32>>,
    // 🧠 Deep Truth: Reasoning Capabilities
    #[serde(default)]
    pub supports_thinking: bool,
    #[serde(default)]
    pub think_tag_schema: String,
    #[serde(default)]
    pub think_end_schema: String,
    #[serde(default)]
    pub reliable_think_close: bool,
}

impl Default for StructuralDNA {
    fn default() -> Self {
        Self {
            model_identity: "unknown".into(),
            layer_count: None,
            attention_head_count: None,
            attention_head_count_kv: None,
            attention_head_dim: None,
            hidden_size: None,
            intermediate_size: None,
            attention_dimensionality_truth: None,
            signature: KernelSignature::default(),
            preferred_runtime: None,
            heterogeneous_map: None,
            max_context_length: None, // Must be truth-grounded
            eos_token: None,
            chat_template: None,
            stop_sequences: Vec::new(),
            inference_params: HashMap::new(),
            dynamic_attributes: HashMap::new(),
            vram_headroom_gb: 0.0,
            ram_headroom_gb: 0.0,
            requires_gpu: false,
            weights_size_gb: 0.0,
            weights_already_loaded: false,
            guidance_bias: None,
            supports_thinking: false,
            think_tag_schema: String::new(),
            think_end_schema: String::new(),
            reliable_think_close: true,
        }
    }
}

// ─── Neural Resource Constants ─────────────────────────────────────────────
const VRAM_CTX_MULTIPLIER: f32 = 4096.0;
const MIN_CONTEXT_FACTOR: usize = 4; // 25% for stability
const DEFAULT_COMPRESSION: f32 = 4.0; // Q4 Standard

impl StructuralDNA {
    /// Single Source of Truth for all reasoning delimiters across Cluaiz (DRY Protocol)
    pub const KNOWN_REASONING_DELIMITERS: &'static [(&'static str, &'static str)] = &[
        ("<think>", "</think>"),
        ("<thought>", "</thought>"),
        ("<|thought|>", "</|thought|>"),
        ("<|start_thought|>", "</|end_thought|>"),
        ("<reasoning>", "</reasoning>"),
        ("[THINK]", "[/THINK]"),
        ("<|begin_thought|>", "<|end_thought|>"),
    ];

    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read DNA: {e}"))?;
        serde_json::from_str(&content).map_err(|e| format!("DNA Syntax Error: {e}"))
    }

    pub fn load_archived(path: &std::path::Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Failed to read Binary DNA: {e}"))?;
        let archived = unsafe { rkyv::archived_root::<StructuralDNA>(&bytes) };
        let deserialized: StructuralDNA = archived.deserialize(&mut rkyv::Infallible).unwrap();
        Ok(deserialized)
    }

    /// Dynamically analyzes chat template to extract reasoning start/end markers
    /// inspired by upstream auto-parser architecture (zero hardcoding).
    pub fn extract_reasoning_markers(template: &str) -> (Option<String>, Option<String>) {
        if template.is_empty() {
            return (None, None);
        }

        // 1. Template variable analysis for reasoning content
        // Jinja: `<think>{{ message.reasoning_content }}</think>` or similar
        if let Some(pos) = template.find("reasoning_content") {
            let before = &template[..pos];
            let after = &template[pos + "reasoning_content".len()..];

            // Extract closing tag from after: find first closing XML/bracket tag in `after`
            let end_tag = if let Some(close_tag_start) = after.find("</") {
                if let Some(close_tag_end) = after[close_tag_start..].find('>') {
                    Some(after[close_tag_start..=close_tag_start + close_tag_end].to_string())
                } else {
                    None
                }
            } else if let Some(close_bracket) = after.find("[/") {
                if let Some(end_bracket) = after[close_bracket..].find(']') {
                    Some(after[close_bracket..=close_bracket + end_bracket].to_string())
                } else {
                    None
                }
            } else {
                None
            };

            // Extract opening tag from before: find tag right before `{{`
            let start_tag = if let Some(open_tag_start) = before.rfind('<') {
                if let Some(open_tag_end) = before[open_tag_start..].find('>') {
                    let tag = &before[open_tag_start..=open_tag_start + open_tag_end];
                    if !tag.starts_with("</") && !tag.contains(' ') && !tag.contains('%') {
                        Some(tag.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else if let Some(open_bracket) = before.rfind('[') {
                if let Some(end_bracket) = before[open_bracket..].find(']') {
                    let tag = &before[open_bracket..=open_bracket + end_bracket];
                    if !tag.starts_with("[/") && !tag.contains(' ') && !tag.contains('%') {
                        Some(tag.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            if start_tag.is_some() || end_tag.is_some() {
                return (start_tag, end_tag);
            }
        }

        // 2. Generic differential scan for standard reasoning delimiter schemas in template
        for &(start, end) in Self::KNOWN_REASONING_DELIMITERS {
            if template.contains(end) || template.contains(start) {
                return (Some(start.to_string()), Some(end.to_string()));
            }
        }

        (None, None)
    }

    /// Separates raw model output into reasoning_content and clean answer content,
    /// matching OpenAI / DeepSeek industry standard payloads.
    pub fn separate_reasoning(
        raw: &str,
        custom_start: Option<&str>,
        custom_end: Option<&str>,
    ) -> (Option<String>, String, usize) {
        if raw.is_empty() {
            return (None, String::new(), 0);
        }

        let marker_candidates: Vec<(&str, &str)> = {
            let mut list = Vec::new();
            if let (Some(cs), Some(ce)) = (custom_start, custom_end) {
                if !cs.is_empty() && !ce.is_empty() {
                    list.push((cs, ce));
                }
            }
            list.extend_from_slice(Self::KNOWN_REASONING_DELIMITERS);
            list
        };

        for (start, end) in marker_candidates {
            if let Some(start_pos) = raw.find(start) {
                let reasoning_start = start_pos + start.len();
                if let Some(end_offset) = raw[reasoning_start..].find(end) {
                    let end_pos = reasoning_start + end_offset;
                    let reasoning = raw[reasoning_start..end_pos].trim().to_string();
                    let before = &raw[..start_pos];
                    let after = &raw[end_pos + end.len()..];
                    let clean = format!("{}{}", before, after).trim().to_string();
                    let tokens = (reasoning.len() / 4).max(reasoning.split_whitespace().count()).max(1);
                    return (Some(reasoning), clean, tokens);
                } else {
                    // Start tag found, but end tag missing (e.g. truncated generation)
                    let reasoning = raw[reasoning_start..].trim().to_string();
                    let clean = raw[..start_pos].trim().to_string();
                    let tokens = (reasoning.len() / 4).max(reasoning.split_whitespace().count()).max(1);
                    return (Some(reasoning), clean, tokens);
                }
            }
        }

        (None, raw.to_string(), 0)
    }

    /// 🧬 Neural Discovery: Learns model behavior and cross-references with Hardware Truth.
    pub fn discover_from_path(&mut self, model_dir: &std::path::Path) -> anyhow::Result<()> {
        crate::dev_info!(
            "🧬 [DNA] Discovery Heartbeat: Investigating -> {:?}",
            model_dir
        );
        let mut arch_limit: Option<usize> = None;
        let mut sliding_window: Option<usize> = None;

        // 🛡️ 0. Hardware Awareness (The Physical Constraints)
        use crate::hardware::governor::HardwareGovernor;

        let opt_control = HardwareGovernor::load_optimization_settings().unwrap_or_default();
        let control = HardwareGovernor::load_system_control()?;

        // 🛡️ Truth Protocol: Prioritize Binary Silicon Truth
        self.vram_headroom_gb = control
            .silicon_truth
            .accelerators
            .gpus
            .iter()
            .map(|g| g.vram_total_gb)
            .sum::<f64>() as f32;
        self.ram_headroom_gb = control.silicon_truth.memory.available_capacity_gb as f32;

        if self.vram_headroom_gb == 0.0 && self.ram_headroom_gb == 0.0 {
            return Err(anyhow::anyhow!(
                "❌ [DNA] Fatal: Hardware Truth Missing or Corrupted. Run 'cluaiz calibrate'."
            ));
        }

        let mut did_probe = false;
        let mut template_opt = None;
        if self.layer_count.is_none() || !self.dynamic_attributes.contains_key("has_native_mtp") {
            let paths_to_check = vec![
                crate::environment::EnvironmentManager::current().model_registry_json_path(),
                crate::environment::EnvironmentManager::current()
                    .local_dir
                    .join("engine")
                    .join("config")
                    .join("model_registry.json"),
            ];
            for reg_path in paths_to_check {
                if reg_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&reg_path) {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(installed) =
                                val.get("installed_models").and_then(|m| m.as_object())
                            {
                                let target_dir_str = model_dir
                                    .to_string_lossy()
                                    .to_lowercase()
                                    .replace('\\', "/");
                                for (_id, entry) in installed {
                                    let local_dir = entry
                                        .get("local_dir")
                                        .and_then(|d| d.as_str())
                                        .unwrap_or("")
                                        .to_lowercase()
                                        .replace('\\', "/");
                                    let primary_file = entry
                                        .get("files")
                                        .and_then(|f| f.as_array())
                                        .and_then(|arr| {
                                            arr.iter().find(|f| {
                                                f.get("is_primary")
                                                    .and_then(|p| p.as_bool())
                                                    .unwrap_or(false)
                                            })
                                        })
                                        .and_then(|f| f.get("name").and_then(|n| n.as_str()))
                                        .unwrap_or("")
                                        .to_lowercase();

                                    let matches = (!local_dir.is_empty()
                                        && (local_dir == target_dir_str
                                            || target_dir_str.contains(&local_dir)
                                            || local_dir.contains(&target_dir_str)))
                                        || (!primary_file.is_empty()
                                            && target_dir_str.contains(&primary_file));

                                    if matches {
                                        if let Some(meta) = entry.get("metadata") {
                                            if let Some(arch) =
                                                meta.get("architecture").and_then(|v| v.as_str())
                                            {
                                                self.model_identity = arch.to_string();
                                            }
                                            if let Some(ctx_str) = meta
                                                .get("context_length")
                                                .or_else(|| meta.get("context_window"))
                                                .and_then(|v| v.as_str())
                                            {
                                                if let Ok(ctx) = ctx_str.parse::<usize>() {
                                                    arch_limit = Some(ctx);
                                                }
                                            }
                                            if let Some(tmpl) =
                                                meta.get("chat_template").and_then(|v| v.as_str())
                                            {
                                                template_opt = Some(tmpl.to_string());
                                            }
                                        }
                                        did_probe = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                if did_probe {
                    break;
                }
            }

            if let Some(template) = template_opt {
                let (st, et) = Self::extract_reasoning_markers(&template);
                if let Some(start) = st {
                    self.supports_thinking = true;
                    self.think_tag_schema = start;
                }
                if let Some(end) = et {
                    self.think_end_schema = end;
                }
                self.chat_template = Some(template);
            }
        }

        // 🛠️ DEEP TRUTH RESOLUTION
        let mut final_truth = if did_probe {
            arch_limit.or(sliding_window)
        } else {
            self.max_context_length
        };

        if final_truth.is_none() {
            return Err(anyhow::anyhow!("❌ [DNA] Fatal: Corrupted Model Metadata."));
        }

        let _ctx = final_truth.unwrap();

        // 🧬 WEIGHT DISCOVERY
        let mut model_size_gb = 0.0;
        let abs_dir = std::fs::canonicalize(model_dir).unwrap_or(model_dir.to_path_buf());

        if let Ok(entries) = std::fs::read_dir(&abs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_lowercase())
                {
                    if ext == "gguf" || ext == "bin" || ext == "safetensors" {
                        model_size_gb += entry.metadata().map(|m| m.len()).unwrap_or(0) as f64
                            / 1024.0
                            / 1024.0
                            / 1024.0;
                    }
                }
            }
        }
        self.weights_size_gb = model_size_gb as f32;

        // 🛡️ 3. Physical VRAM Arbiter (Sovereign Negotiation)
        // Delegate to Governor for real-time fitting.
        // We temporarily set max_context_length so the Governor can see the architecture cap.
        self.max_context_length = final_truth;
        let final_ctx = HardwareGovernor::negotiate_vram_envelope(&self);

        self.max_context_length = Some(final_ctx);

        // 📊 SOVEREIGN TELEMETRY: Synchronize with Governor Truth
        self.dynamic_attributes.insert(
            "context_window".to_string(),
            format!("{}k", final_ctx / 1024),
        );

        let opt_control = HardwareGovernor::load_optimization_settings().unwrap_or_default();

        // 🚀 DYNAMIC QUOTA: Mode-aware allocation (No more 75% static wall)
        let gen_headroom = if opt_control.custom_vram_buffer_gb.is_some() {
            0.95
        } else {
            0.90
        };

        let max_gen_tokens = (final_ctx as f64 * gen_headroom) as usize;
        self.inference_params
            .insert("max_tokens".to_string(), max_gen_tokens.to_string());
        self.inference_params
            .insert("context_length".to_string(), final_ctx.to_string());

        info!(
            "✅ [DNA] Governor Discovery Complete: Buffer {:?} | Window {}k",
            opt_control.custom_vram_buffer_gb,
            final_ctx / 1024
        );

        Ok(())
    }

    /// Truth Protocol: Synchronizes DNA fields with actual binary metadata.
    pub fn sync_with_metadata(
        &mut self,
        metadata: &HashMap<String, String>,
        _tensor_infos: &HashMap<String, Vec<usize>>,
    ) {
        // [SOVEREIGN CLEAN]: Switched to println for better editor compatibility
        println!("🧬 [DNA] Initiating Multi-Layer Truth Protocol...");

        for (key, value) in metadata {
            if key.ends_with(".embedding_length") || key.ends_with(".hidden_size") {
                if let Ok(v) = value.parse::<usize>() {
                    self.hidden_size = Some(v);
                }
            } else if key.ends_with(".block_count") || key.ends_with(".layer_count") {
                if let Ok(v) = value.parse::<usize>() {
                    self.layer_count = Some(v);
                }
            } else if key.ends_with(".attention.head_count")
                || key.ends_with(".num_attention_heads")
            {
                if let Ok(v) = value.parse::<usize>() {
                    self.attention_head_count = Some(v);
                }
            } else if key.ends_with(".attention.head_count_kv")
                || key.ends_with(".num_key_value_heads")
            {
                if let Ok(v) = value.parse::<usize>() {
                    self.attention_head_count_kv = Some(v);
                }
            } else if key.ends_with(".feed_forward_length") || key.ends_with(".intermediate_size") {
                if let Ok(v) = value.parse::<usize>() {
                    self.intermediate_size = Some(v);
                }
            } else if key.contains("context_length") || key.contains("max_position_embeddings") {
                if let Ok(v) = value.parse::<usize>() {
                    self.max_context_length = Some(v);
                }
            } else if key == "general.architecture" {
                self.model_identity = value.clone();
            }
        }
    }

    /// 🛠️ Parser: Converts manifest context strings (e.g., "8k", "128k") to usize.
    pub fn parse_context_string(ctx_str: &str) -> usize {
        let normalized = ctx_str.to_lowercase();
        if normalized.ends_with('k') {
            let num = normalized
                .trim_end_matches('k')
                .parse::<usize>()
                .unwrap_or(4);
            num * 1024
        } else if normalized.ends_with('m') {
            let num = normalized
                .trim_end_matches('m')
                .parse::<usize>()
                .unwrap_or(1);
            num * 1024 * 1024
        } else {
            normalized.parse::<usize>().unwrap_or(2048)
        }
    }

    /// 🛠️ Skeleton Factory: Creates a primed DNA backbone from manifest data.
    pub fn create_skeleton(
        id: String,
        has_vision: bool,
        expert_count: Option<usize>,
        bit_depth: f64,
        context_window: &str,
    ) -> Self {
        let mut signature = KernelSignature::default();
        signature.is_multimodal = has_vision;
        if expert_count.is_some() {
            signature.has_experts = true;
        }

        let mut preferred_runtime = Some(BackendType::RuntimeA); // Default: Candle
        if bit_depth < 2.0 {
            signature.is_bitnet = true;
            preferred_runtime = Some(BackendType::RuntimeB); // BitNet -> Llama.cpp
        }

        Self {
            model_identity: id,
            signature,
            preferred_runtime,
            max_context_length: Some(Self::parse_context_string(context_window)),
            ..Default::default()
        }
    }

    /// Dynamic Architectural Truth: Determines whether this model requires pure F16 KV cache
    pub fn requires_fp16_kv(&self) -> bool {
        self.signature.requires_fp16_kv(self.attention_head_dim)
            || self.dynamic_attributes.contains_key("is_ssm")
            || self.dynamic_attributes.contains_key("has_softcapping")
    }

    /// Dynamic Architectural Truth: Determines whether Flash Attention is valid for this model without divergence
    pub fn supports_flash_attention(&self) -> bool {
        self.signature
            .supports_flash_attention(self.attention_head_dim)
            && !self.dynamic_attributes.contains_key("is_ssm")
    }
}
