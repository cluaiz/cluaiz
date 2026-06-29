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

        let booster = HardwareGovernor::load_booster_settings().unwrap_or_default();
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

        // 🛡️ Manifest Validation: Check if model REQUIRES GPU
        let manifest_path = model_dir.join("model_manifest.json");
        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                self.requires_gpu = json
                    .get("requires_gpu")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            }
        }

        if self.requires_gpu && self.vram_headroom_gb == 0.0 {
            return Err(anyhow::anyhow!("❌ [DNA] Hardware Mismatch: This model requires a GPU but none was detected or available. Aborting to prevent freeze."));
        }

        if self.vram_headroom_gb == 0.0 && self.ram_headroom_gb == 0.0 {
            return Err(anyhow::anyhow!(
                "❌ [DNA] Fatal: Hardware Truth Missing or Corrupted. Run 'cluaiz calibrate'."
            ));
        }

        // 1. Native GGUF Probe (The REAL Truth - No external JSONs needed)
        let mut gguf_path = None;
        if let Ok(entries) = std::fs::read_dir(model_dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        let p = entry.path();
                        if p.extension().and_then(|e| e.to_str()) == Some("gguf") {
                            gguf_path = Some(p);
                            break;
                        }
                    }
                }
            }
        }

        if let Some(path) = gguf_path {
            if let Ok((metadata, _tensor_infos, _count)) = crate::utils::GGUFProber::probe(&path) {
                if let Some(arch) = metadata.get("general.architecture") {
                    self.model_identity = arch.clone();

                    let ctx_key = format!("{}.context_length", arch);
                    if let Some(ctx_str) = metadata.get(&ctx_key) {
                        if let Ok(ctx) = ctx_str.parse::<usize>() {
                            arch_limit = Some(ctx);
                        }
                    }

                    if let Some(val) = metadata
                        .get(&format!("{}.block_count", arch))
                        .and_then(|v| v.parse::<usize>().ok())
                    {
                        self.layer_count = Some(val);
                    }
                    if let Some(val) = metadata
                        .get(&format!("{}.attention.head_count", arch))
                        .and_then(|v| v.parse::<usize>().ok())
                    {
                        self.attention_head_count = Some(val);
                    }
                    if let Some(val) = metadata
                        .get(&format!("{}.attention.head_count_kv", arch))
                        .and_then(|v| v.parse::<usize>().ok())
                    {
                        self.attention_head_count_kv = Some(val);
                    }
                    if let Some(val) = metadata
                        .get(&format!("{}.feed_forward_length", arch))
                        .and_then(|v| v.parse::<usize>().ok())
                    {
                        self.intermediate_size = Some(val);
                    }
                    if let Some(val) = metadata
                        .get(&format!("{}.embedding_length", arch))
                        .and_then(|v| v.parse::<usize>().ok())
                    {
                        self.hidden_size = Some(val);
                    }
                }

                // 2. Read tokenizer.chat_template to detect reasoning natively
                if let Some(template) = metadata.get("tokenizer.chat_template") {
                    self.chat_template = Some(template.clone());

                    let template_lower = template.to_lowercase();
                    let mut detected_start = None;
                    let mut detected_end = None;

                    let keywords = [
                        "think",
                        "thought",
                        "reasoning",
                        "reason",
                        "brainstorm",
                        "logic",
                    ];
                    for kw in keywords.iter() {
                        let formats = [
                            (format!("<{}>", kw), format!("</{}>", kw)),
                            (format!("<|{}_start|>", kw), format!("<|{}_end|>", kw)),
                            (format!("<|{}|>", kw), format!("</|{}|>", kw)),
                            (format!("<|channel>{}", kw), format!("<channel|>")),
                        ];

                        for (start, end) in formats.iter() {
                            if template_lower.contains(start) {
                                detected_start = Some(start.clone());
                                if template_lower.contains(end) {
                                    detected_end = Some(end.clone());
                                }
                                break;
                            }
                        }
                        if detected_start.is_some() {
                            break;
                        }
                    }

                    if let Some(start_tag) = detected_start {
                        self.supports_thinking = true;
                        self.think_tag_schema = start_tag.clone();
                        
                        if let Some(end_tag) = detected_end.clone() {
                            self.think_end_schema = end_tag;
                            self.reliable_think_close = true;
                        } else {
                            if start_tag.contains("_start") {
                                self.think_end_schema = start_tag.replace("_start", "_end");
                            } else if start_tag.contains("<|") {
                                self.think_end_schema = start_tag.replace("<|", "</|");
                            } else {
                                self.think_end_schema = start_tag.replace("<", "</");
                            }
                            self.reliable_think_close = false;
                        }
                        
                        crate::dev_info!(
                            "🧠 [DNA] Universal Native Truth: Reasoning Model Detected (Start: {}, End: {})",
                            self.think_tag_schema,
                            self.think_end_schema
                        );
                    }
                }

                // The engine's native backend (candle/llama.cpp) will automatically handle the EOS token ID
                // extracted from the GGUF header (tokenizer.ggml.eos_token_id) during generation.
                // Hardcoding architecture names here for string-based stop sequences defeats the Deep Truth native approach.
            } else {
                eprintln!("⚠️ [DNA] GGUF probe failed on: {:?}", path);
            }
        } else {
            eprintln!("⚠️ [DNA] No .gguf file found in model directory for probe.");
        }

        // 🛠️ DEEP TRUTH RESOLUTION
        let mut final_truth = arch_limit.or(sliding_window);

        // Rule: If manual DNA exists, prioritize it but CAP by Architecture to prevent Hallucinations.
        let dna_json_path = model_dir.join("structural_dna.json");
        if let Ok(content) = std::fs::read_to_string(&dna_json_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                // Try root max_context_length first, then dynamic_attributes
                let dna_ctx_val = json.get("max_context_length").or_else(|| {
                    json.get("dynamic_attributes")
                        .and_then(|d| d.get("context_window"))
                });

                if let Some(val) = dna_ctx_val {
                    let dna_ctx = if let Some(ctx_u) = val.as_u64() {
                        ctx_u as usize
                    } else if let Some(ctx_s) = val.as_str() {
                        Self::parse_context_string(ctx_s)
                    } else {
                        0
                    };

                    if dna_ctx > 0 {
                        if let Some(arch_ctx) = final_truth {
                            final_truth = Some(dna_ctx.min(arch_ctx));
                        } else {
                            final_truth = Some(dna_ctx);
                        }
                    }
                }
            }
        }

        if final_truth.is_none() {
            return Err(anyhow::anyhow!("❌ [DNA] Fatal: Corrupted Model Metadata."));
        }

        let _ctx = final_truth.unwrap();

        // 🧬 SOVEREIGN WEIGHT DISCOVERY
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

        // 🚀 DYNAMIC QUOTA: Mode-aware allocation (No more 75% static wall)
        let gen_headroom = match booster.mode_run {
            crate::hardware::schema::booster::BoosterMode::UltraMaxBoost
            | crate::hardware::schema::booster::BoosterMode::HyperCluster => 0.95, // 95% for Extreme modes
            crate::hardware::schema::booster::BoosterMode::MaxBoost => 0.90, // 90%
            _ => 0.80,                                                       // 80% Standard
        };

        let max_gen_tokens = (final_ctx as f64 * gen_headroom) as usize;
        self.inference_params
            .insert("max_tokens".to_string(), max_gen_tokens.to_string());
        self.inference_params
            .insert("context_length".to_string(), final_ctx.to_string());

        info!(
            "✅ [DNA] Governor Discovery Complete: Mode {:?} | Window {}k",
            booster.mode_run,
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
            normalized.parse::<usize>().unwrap_or(4096)
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
}
