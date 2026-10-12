//! ⚙️ Centralized Settings & User Profiles for Cluaiz Engine
//! Single Source of Truth for system configuration, hardware preferences, and optimization switches.

use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use once_cell::sync::Lazy;
use crate::env::EnvironmentManager;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum FeatureState {
    #[serde(alias = "on")]
    On,
    #[serde(alias = "off")]
    Off,
    #[default]
    #[serde(alias = "auto")]
    Auto,
}

impl FeatureState {
    pub fn is_active(&self) -> bool {
        matches!(self, FeatureState::On | FeatureState::Auto)
    }

    /// Lowercase name ("on", "off", "auto"): the form chat requests and the CLI use
    pub fn name(&self) -> &'static str {
        match self {
            FeatureState::On => "on",
            FeatureState::Off => "off",
            FeatureState::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum KvCacheQuantization {
    #[default]
    #[serde(rename = "Auto", alias = "auto")]
    Auto,
    #[serde(rename = "Kv16", alias = "kv16")]
    Kv16,
    #[serde(rename = "Kv8", alias = "kv8")]
    Kv8,
    #[serde(rename = "Kv4", alias = "kv4")]
    Kv4,
}

impl KvCacheQuantization {
    /// Maps KvCacheQuantization setting to upstream llama.cpp GGML types (type_k, type_v).
    /// GGML_TYPE_F16 = 1, GGML_TYPE_Q8_0 = 8, GGML_TYPE_Q4_0 = 2.
    pub fn to_ggml_types(&self) -> (i32, i32) {
        match self {
            KvCacheQuantization::Kv4 => (2, 2),  // GGML_TYPE_Q4_0
            KvCacheQuantization::Kv8 => (8, 8),  // GGML_TYPE_Q8_0
            KvCacheQuantization::Kv16 => (1, 1), // GGML_TYPE_F16
            KvCacheQuantization::Auto => (1, 1), // Default upstream F16
        }
    }

    /// Exact memory bytes per KV element based on upstream GGML block geometry.
    /// Q4_0: 18 bytes / 32 elements = 0.5625 B/element
    /// Q8_0: 34 bytes / 32 elements = 1.0625 B/element
    /// F16:  2.0 bytes / element
    pub fn bytes_per_element(&self) -> f64 {
        match self {
            KvCacheQuantization::Kv4 => 18.0 / 32.0,
            KvCacheQuantization::Kv8 => 34.0 / 32.0,
            KvCacheQuantization::Kv16 | KvCacheQuantization::Auto => 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum ContextShiftingMode {
    #[default]
    #[serde(rename = "Auto", alias = "auto")]
    Auto,
    #[serde(rename = "Off", alias = "off")]
    Off,
    #[serde(rename = "Minimal", alias = "minimal")]
    Minimal,
    #[serde(rename = "Standard", alias = "standard", alias = "on", alias = "On")]
    Standard,
    #[serde(rename = "Aggressive", alias = "aggressive")]
    Aggressive,
    #[serde(rename = "Extreme", alias = "extreme")]
    Extreme,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptimizationControl {
    #[serde(default)]
    pub flash_attention: FeatureState,
    #[serde(default)]
    pub kv_cache_quantization: KvCacheQuantization,
    #[serde(default)]
    pub context_shifting: ContextShiftingMode,
    /// Direct GB safety buffer override for VRAM. None = dynamic auto mode.
    #[serde(default)]
    pub custom_vram_buffer_gb: Option<f64>,
    /// Direct GB safety buffer override for CPU RAM. None = dynamic auto mode.
    #[serde(default)]
    pub custom_ram_buffer_gb: Option<f64>,
    /// Number of GPU layers to offload. None = Auto dynamic, Some(0) = CPU Only, Some(-1) = Full GPU
    #[serde(default)]
    pub n_gpu_layers: Option<i32>,
    /// Thinking mode for reasoning models (Auto / On / Off)
    #[serde(default)]
    pub think_mode: FeatureState,
    /// Context batch processing size (tokens per batch). None = dynamic auto mode (512).
    #[serde(default)]
    pub batch_size: Option<u32>,
    /// Physical micro-batch evaluation chunk size. None = dynamic auto mode (512).
    #[serde(default)]
    pub ubatch_size: Option<u32>,
    /// Memory mapping flag for GGUF model files. None = dynamic auto mode (true).
    #[serde(default)]
    pub use_mmap: Option<bool>,
    /// Direct context window override in tokens. None = dynamic auto derivation, Some(4096) = exact 4096 tokens, Some(0) = max native context.
    #[serde(default, alias = "custom_n_ctx", alias = "n_ctx")]
    pub custom_context_window: Option<u32>,
    /// llama.cpp `--n-cpu-moe`: experts of the first N blocks stay in RAM. None = planner decides.
    #[serde(default)]
    pub n_cpu_moe: Option<u32>,
    /// llama.cpp `offload_kqv`: false keeps the whole KV cache in RAM. None = planner decides.
    #[serde(default)]
    pub offload_kqv: Option<bool>,
    /// CPU threads for generation. None = physical cores (llama.cpp CLI default).
    #[serde(default)]
    pub n_threads: Option<i32>,
    /// CPU threads for prompt processing. None = same as `n_threads` (llama.cpp CLI default).
    #[serde(default)]
    pub n_threads_batch: Option<i32>,
    /// llama.cpp `op_offload` (`--no-op-offload` = false): large batches of CPU-held weights are
    /// copied to the GPU to compute there. None = llama.cpp default (on when any layer is on the GPU).
    #[serde(default)]
    pub op_offload: Option<bool>,
    /// Default thinking budget ("Effort") for chats that do not send `reasoning_effort`.
    /// Per request, not a load setting: changing it does not reload the model.
    #[serde(default)]
    pub reasoning_effort: ThinkingBudget,
    /// Share of the room and maximum tokens for Low / Medium / High (Auto = High)
    #[serde(default)]
    pub thinking_presets: ThinkingPresets,
    /// Text written before the end-of-thinking tag when the budget runs out (llama.cpp
    /// `--reasoning-budget-message`), so the model wraps up instead of stopping mid-thought. None = no text.
    #[serde(default)]
    pub reasoning_budget_message: Option<String>,
}

/// Thinking token budget ("Effort" in the app): how many tokens a reasoning model may think before it must
/// answer. Accepts the names below or a number of tokens (as a JSON number or a numeric string).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkingBudget {
    /// Same as High
    #[default]
    Auto,
    Low,
    Medium,
    High,
    /// All of the room (see `ThinkingBudget::limit`)
    Max,
    /// This many thinking tokens, never more than the room (0 = answer without thinking)
    Tokens(u32),
}

/// One effort level: a share of the room for thinking, never more than `max_tokens`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ThinkingShare {
    /// 0.0 - 1.0 of the room
    pub share: f64,
    pub max_tokens: u32,
}

/// What Low / Medium / High mean (Auto uses High). User-editable defaults, not fixed in the engine.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ThinkingPresets {
    pub low: ThinkingShare,
    pub medium: ThinkingShare,
    pub high: ThinkingShare,
}

impl Default for ThinkingPresets {
    fn default() -> Self {
        Self {
            low: ThinkingShare { share: 0.25, max_tokens: 2048 },
            medium: ThinkingShare { share: 0.50, max_tokens: 8192 },
            high: ThinkingShare { share: 0.75, max_tokens: 32768 },
        }
    }
}

/// Thinking budget for one generation, after the room limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ThinkingLimit {
    pub tokens: usize,
    /// The chosen budget was bigger than the room and was lowered so the answer still fits
    pub capped: bool,
}

impl ThinkingBudget {
    /// Thinking tokens for one generation. `room` = context left after the prompt minus the tokens kept
    /// for the answer, so whatever is chosen, the answer still fits (room 0 = no thinking).
    pub fn limit(&self, room: usize, presets: &ThinkingPresets) -> ThinkingLimit {
        let of_room = |p: &ThinkingShare| {
            ((room as f64 * p.share.clamp(0.0, 1.0)) as usize).min(p.max_tokens as usize)
        };
        match self {
            ThinkingBudget::Auto | ThinkingBudget::High => ThinkingLimit { tokens: of_room(&presets.high), capped: false },
            ThinkingBudget::Low => ThinkingLimit { tokens: of_room(&presets.low), capped: false },
            ThinkingBudget::Medium => ThinkingLimit { tokens: of_room(&presets.medium), capped: false },
            ThinkingBudget::Max => ThinkingLimit { tokens: room, capped: false },
            ThinkingBudget::Tokens(n) => ThinkingLimit { tokens: (*n as usize).min(room), capped: *n as usize > room },
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_lowercase().as_str() {
            "auto" | "" => Ok(ThinkingBudget::Auto),
            "low" | "minimal" => Ok(ThinkingBudget::Low),
            "medium" | "standard" => Ok(ThinkingBudget::Medium),
            "high" => Ok(ThinkingBudget::High),
            "max" | "extreme" => Ok(ThinkingBudget::Max),
            n => n.parse::<u32>().map(ThinkingBudget::Tokens).map_err(|_| {
                format!("reasoning_effort '{}': use auto, low, medium, high, max or a number of tokens", value)
            }),
        }
    }
}

impl Serialize for ThinkingBudget {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            ThinkingBudget::Auto => s.serialize_str("auto"),
            ThinkingBudget::Low => s.serialize_str("low"),
            ThinkingBudget::Medium => s.serialize_str("medium"),
            ThinkingBudget::High => s.serialize_str("high"),
            ThinkingBudget::Max => s.serialize_str("max"),
            ThinkingBudget::Tokens(n) => s.serialize_u32(*n),
        }
    }
}

impl<'de> Deserialize<'de> for ThinkingBudget {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::String(s) => ThinkingBudget::parse(&s).map_err(serde::de::Error::custom),
            serde_json::Value::Number(n) => n
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .map(ThinkingBudget::Tokens)
                .ok_or_else(|| serde::de::Error::custom("reasoning_effort must be a whole number of tokens >= 0")),
            serde_json::Value::Null => Ok(ThinkingBudget::Auto),
            other => Err(serde::de::Error::custom(format!(
                "reasoning_effort {}: use auto, low, medium, high, max or a number of tokens", other
            ))),
        }
    }
}

/// Field names of `OptimizationControl` (checked against serde's output by a unit test).
const OPTIMIZATION_FIELDS: &[&str] = &[
    "flash_attention", "kv_cache_quantization", "context_shifting", "custom_vram_buffer_gb",
    "custom_ram_buffer_gb", "n_gpu_layers", "think_mode", "batch_size", "ubatch_size", "use_mmap",
    "custom_context_window", "n_cpu_moe", "offload_kqv", "n_threads", "n_threads_batch", "op_offload",
    "reasoning_effort", "thinking_presets", "reasoning_budget_message",
];

/// Context window setting, same meaning as llama.cpp `n_ctx`: 0 = Auto, -1 = Max (native), N = exactly N.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextSetting {
    Auto,
    Max,
    Custom(u32),
}

impl ContextSetting {
    /// Parses a llama.cpp style `n_ctx` value (0 = Auto, negative = Max, N = Custom).
    pub fn from_llama_n_ctx(n_ctx: i64) -> Self {
        match n_ctx {
            0 => ContextSetting::Auto,
            n if n < 0 => ContextSetting::Max,
            n => ContextSetting::Custom(n.min(u32::MAX as i64) as u32),
        }
    }

    /// Returns the llama.cpp style `n_ctx` value (0 = Auto, -1 = Max, N = Custom).
    pub fn to_llama_n_ctx(&self) -> i64 {
        match self {
            ContextSetting::Auto => 0,
            ContextSetting::Max => -1,
            ContextSetting::Custom(n) => *n as i64,
        }
    }
}

/// Default sampler values used when a chat request does not send its own (llama.cpp names).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SamplerDefaults {
    #[serde(default)]
    pub temp: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub top_k: Option<i32>,
    #[serde(default)]
    pub min_p: Option<f32>,
    #[serde(default)]
    pub presence_penalty: Option<f32>,
    #[serde(default)]
    pub frequency_penalty: Option<f32>,
    #[serde(default)]
    pub repeat_penalty: Option<f32>,
    #[serde(default)]
    pub seed: Option<u32>,
}

/// Loaded chat model (path, context size llama.cpp actually created). None = nothing loaded.
/// Engine and API live in one process, so this is the single runtime truth for UI and preflight.
static ACTIVE_MODEL_CONTEXT: Lazy<RwLock<Option<(String, u32)>>> = Lazy::new(|| RwLock::new(None));

/// Records the loaded model and its real context size (None when unloaded).
pub fn set_active_context(model: Option<(String, u32)>) {
    *ACTIVE_MODEL_CONTEXT.write().unwrap() = model;
}

/// Context size of the loaded model, or None when no model is loaded.
pub fn active_context_tokens() -> Option<u32> {
    ACTIVE_MODEL_CONTEXT.read().unwrap().as_ref().map(|(_, n)| *n)
}

/// Path of the loaded model file, or None when no model is loaded.
pub fn active_model_path() -> Option<String> {
    ACTIVE_MODEL_CONTEXT.read().unwrap().as_ref().map(|(p, _)| p.clone())
}

pub const DEFAULT_SAFE_VRAM_BUFFER_GB: f64 = 1.0; // 1024 MiB standard buffer rule
pub const MIN_SAFE_VRAM_BUFFER_GB: f64 = 0.50; // 512 MiB second candidate (BUFFER rules)
pub const DEFAULT_SAFE_RAM_BUFFER_GB: f64 = 1.0;
pub const MIN_SAFE_RAM_BUFFER_GB: f64 = 0.50; // 512 MiB minimal safe headroom to prevent OS lockup

impl OptimizationControl {
    /// Resolves static VRAM safety buffer in GB (using custom override or engine default)
    pub fn resolved_vram_buffer_gb(&self) -> f64 {
        self.custom_vram_buffer_gb.unwrap_or(DEFAULT_SAFE_VRAM_BUFFER_GB)
    }

    /// Dynamically resolves the optimal VRAM safety buffer in GB:
    /// - If user specified a custom override, respect it.
    /// - If ample headroom (>= 500 MB), use standard 500 MB buffer.
    /// - If tight, but reducing buffer down to 200-250 MB allows model + KV cache to fit 100% on GPU,
    ///   dynamically scale down buffer to keep all layers on GPU and prevent 95% CPU-PCIe slowdown!
    pub fn adaptive_vram_buffer_gb(&self, free_vram_gb: f64, required_vram_gb: f64) -> f64 {
        if let Some(custom) = self.custom_vram_buffer_gb {
            return custom;
        }
        let headroom = free_vram_gb - required_vram_gb;
        if headroom >= DEFAULT_SAFE_VRAM_BUFFER_GB {
            DEFAULT_SAFE_VRAM_BUFFER_GB
        } else if headroom >= MIN_SAFE_VRAM_BUFFER_GB {
            headroom.clamp(MIN_SAFE_VRAM_BUFFER_GB, DEFAULT_SAFE_VRAM_BUFFER_GB)
        } else {
            MIN_SAFE_VRAM_BUFFER_GB
        }
    }

    /// Resolves static RAM safety buffer in GB (using custom override or engine default)
    pub fn resolved_ram_buffer_gb(&self) -> f64 {
        self.custom_ram_buffer_gb.unwrap_or(DEFAULT_SAFE_RAM_BUFFER_GB)
    }

    /// Dynamically resolves the optimal RAM safety buffer in GB:
    /// - If user specified a custom override, respect it.
    /// - If ample headroom (>= 1.0 GB), use standard 1.0 GB buffer.
    /// - If tight, but reducing buffer down to 300-500 MB allows model/experts to fit in RAM,
    ///   scale down buffer to keep everything in RAM and prevent slow SSD disk thrashing!
    pub fn adaptive_ram_buffer_gb(&self, available_ram_gb: f64, required_ram_gb: f64) -> f64 {
        if let Some(custom) = self.custom_ram_buffer_gb {
            return custom;
        }
        let headroom = available_ram_gb - required_ram_gb;
        if headroom >= DEFAULT_SAFE_RAM_BUFFER_GB {
            DEFAULT_SAFE_RAM_BUFFER_GB
        } else if headroom >= MIN_SAFE_RAM_BUFFER_GB {
            headroom.clamp(MIN_SAFE_RAM_BUFFER_GB, DEFAULT_SAFE_RAM_BUFFER_GB)
        } else {
            MIN_SAFE_RAM_BUFFER_GB
        }
    }

    /// Context setting: None = Auto, Some(0) = Max (native), Some(N) = exactly N.
    pub fn context_setting(&self) -> ContextSetting {
        match self.custom_context_window {
            None => ContextSetting::Auto,
            Some(0) | Some(u32::MAX) => ContextSetting::Max,
            Some(n) => ContextSetting::Custom(n),
        }
    }

    pub fn set_context_setting(&mut self, setting: ContextSetting) {
        self.custom_context_window = match setting {
            ContextSetting::Auto => None,
            ContextSetting::Max => Some(0),
            ContextSetting::Custom(n) => Some(n),
        };
    }

    /// Load-time parameters that change how llama.cpp builds the model/context.
    /// When this string changes, the model must be reloaded; sampler settings are not part of it.
    pub fn load_fingerprint(&self) -> String {
        format!(
            "ctx={:?}|ngl={:?}|ncmoe={:?}|kqv={:?}|opo={:?}|kv={:?}|fa={:?}|b={}|ub={}|mmap={}|vbuf={:?}|t={:?}/{:?}",
            self.context_setting(),
            self.n_gpu_layers,
            self.n_cpu_moe,
            self.offload_kqv,
            self.op_offload,
            self.kv_cache_quantization,
            self.flash_attention,
            self.resolved_batch_size(),
            self.resolved_ubatch_size(),
            self.resolved_use_mmap(),
            self.custom_vram_buffer_gb,
            self.n_threads,
            self.n_threads_batch,
        )
    }

    /// True when the user fixed the placement (n_gpu_layers / n_cpu_moe / offload_kqv):
    /// the planner then uses exactly these values instead of choosing a candidate.
    pub fn is_manual_placement(&self) -> bool {
        self.n_gpu_layers.is_some() || self.n_cpu_moe.is_some() || self.offload_kqv.is_some()
    }

    /// Applies a partial update (only the keys a client changed) on top of these settings.
    /// Keys may use a field's serde alias; the patch value then replaces the saved one.
    pub fn merged_with(&self, patch: &serde_json::Value) -> Result<Self, String> {
        let patch = patch.as_object().ok_or("settings update must be a JSON object")?;
        let mut merged = serde_json::to_value(self).map_err(|e| e.to_string())?;
        // Only settings that exist: an unknown key is a typo or a removed setting, never silently ignored
        let known = Self::default().merged_keys_with_aliases();
        if let Some(unknown) = patch.keys().find(|k| !known.contains(&k.as_str())) {
            return Err(format!("unknown setting `{}`", unknown));
        }
        let fields = merged.as_object_mut().ok_or("settings are not a JSON object")?;
        for (key, value) in patch {
            fields.insert(key.clone(), value.clone());
        }
        // An alias in the patch and the saved canonical key are the same field to serde ("duplicate
        // field `x`"): drop the saved key named in that error and retry; serde decides, no alias list here.
        for _ in 0..=patch.len() {
            match serde_json::from_value::<Self>(merged.clone()) {
                Ok(settings) => return Ok(settings),
                Err(e) => {
                    let msg = e.to_string();
                    let duplicate = msg
                        .strip_prefix("duplicate field `")
                        .and_then(|rest| rest.split('`').next())
                        .filter(|field| !patch.contains_key(*field))
                        .map(str::to_string);
                    let removed = match (duplicate, merged.as_object_mut()) {
                        (Some(field), Some(fields)) => fields.remove(&field).is_some(),
                        _ => false,
                    };
                    if !removed {
                        return Err(msg);
                    }
                }
            }
        }
        Err("settings update could not be applied".into())
    }

    /// Every key a settings update may use: the field names plus their serde aliases.
    fn merged_keys_with_aliases(&self) -> Vec<&'static str> {
        let mut keys: Vec<&'static str> = OPTIMIZATION_FIELDS.to_vec();
        keys.extend_from_slice(&["custom_n_ctx", "n_ctx"]); // aliases of custom_context_window
        keys
    }

    /// (n_threads, n_threads_batch) like llama.cpp's CLI: physical cores, batch = generation threads.
    pub fn resolved_threads(&self, physical_cores: usize) -> (i32, i32) {
        let n_threads = self.n_threads.filter(|n| *n > 0).unwrap_or(physical_cores.max(1) as i32);
        (n_threads, self.n_threads_batch.filter(|n| *n > 0).unwrap_or(n_threads))
    }

    /// Resolves effective batch size for prompt evaluation (default: 512)
    pub fn resolved_batch_size(&self) -> u32 {
        self.batch_size.unwrap_or(512)
    }

    /// Resolves effective micro-batch size for token evaluation (default: 512)
    pub fn resolved_ubatch_size(&self) -> u32 {
        self.ubatch_size.unwrap_or(512)
    }

    /// Resolves whether memory mapping (mmap) is enabled (default: true for instant zero-copy loading)
    pub fn resolved_use_mmap(&self) -> bool {
        self.use_mmap.unwrap_or(true)
    }
}

impl Default for OptimizationControl {
    fn default() -> Self {
        Self {
            flash_attention: FeatureState::Auto,
            kv_cache_quantization: KvCacheQuantization::Auto,
            context_shifting: ContextShiftingMode::Auto,
            custom_vram_buffer_gb: None,
            custom_ram_buffer_gb: None,
            n_gpu_layers: None,
            think_mode: FeatureState::Auto,
            batch_size: None,
            ubatch_size: None,
            use_mmap: None,
            custom_context_window: None,
            n_cpu_moe: None,
            offload_kqv: None,
            n_threads: None,
            n_threads_batch: None,
            op_offload: None,
            reasoning_effort: ThinkingBudget::Auto,
            thinking_presets: ThinkingPresets::default(),
            reasoning_budget_message: None,
        }
    }
}

/// llama.cpp load settings sent with one chat request (`"llama": {...}`), llama.cpp names.
/// Every field is optional: a missing field keeps the user's settings / the planner's auto choice.
/// Setting n_gpu_layers, n_cpu_moe or offload_kqv bypasses the placement planner.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LlamaLoadParams {
    /// 0 = Auto, -1 = Max (native), N = exactly N tokens
    pub n_ctx: Option<i64>,
    #[serde(alias = "ngl")]
    pub n_gpu_layers: Option<i32>,
    pub n_cpu_moe: Option<u32>,
    /// "auto", "f16", "q8_0", "q4_0"
    pub cache_type_k: Option<String>,
    pub cache_type_v: Option<String>,
    /// "auto", "on", "off" (or true / false)
    pub flash_attn: Option<serde_json::Value>,
    pub n_batch: Option<u32>,
    pub n_ubatch: Option<u32>,
    pub n_threads: Option<i32>,
    pub n_threads_batch: Option<i32>,
    pub use_mmap: Option<bool>,
    pub offload_kqv: Option<bool>,
    pub op_offload: Option<bool>,
    /// VRAM kept free in GB (replaces the 1024 / 512 MiB rule)
    pub vram_buffer_gb: Option<f64>,
}

impl LlamaLoadParams {
    /// Writes the given values over `opt`. Errors name the field and the accepted values.
    pub fn apply_to(&self, opt: &mut OptimizationControl) -> Result<(), String> {
        if let Some(n) = self.n_ctx {
            opt.set_context_setting(ContextSetting::from_llama_n_ctx(n));
        }
        if let Some(n) = self.n_gpu_layers {
            opt.n_gpu_layers = Some(n);
        }
        if let Some(n) = self.n_cpu_moe {
            opt.n_cpu_moe = Some(n);
        }
        let k = self.cache_type_k.as_deref().map(parse_cache_type).transpose()?;
        let v = self.cache_type_v.as_deref().map(parse_cache_type).transpose()?;
        match (k, v) {
            (Some(k), Some(v)) if k != v => {
                return Err("cache_type_k and cache_type_v must be the same type (different K/V types are not implemented)".into())
            }
            (Some(t), _) | (None, Some(t)) => opt.kv_cache_quantization = t,
            (None, None) => {}
        }
        if let Some(fa) = &self.flash_attn {
            opt.flash_attention = match fa {
                serde_json::Value::Bool(true) => FeatureState::On,
                serde_json::Value::Bool(false) => FeatureState::Off,
                serde_json::Value::String(s) => match s.to_lowercase().as_str() {
                    "on" | "1" | "true" => FeatureState::On,
                    "off" | "0" | "false" => FeatureState::Off,
                    "auto" | "-1" => FeatureState::Auto,
                    other => return Err(format!("flash_attn '{}': use \"auto\", \"on\" or \"off\"", other)),
                },
                other => return Err(format!("flash_attn {}: use \"auto\", \"on\" or \"off\"", other)),
            };
        }
        if let Some(n) = self.n_batch {
            opt.batch_size = Some(n);
        }
        if let Some(n) = self.n_ubatch {
            opt.ubatch_size = Some(n);
        }
        if let Some(n) = self.n_threads {
            opt.n_threads = Some(n);
        }
        if let Some(n) = self.n_threads_batch {
            opt.n_threads_batch = Some(n);
        }
        if let Some(m) = self.use_mmap {
            opt.use_mmap = Some(m);
        }
        if let Some(o) = self.offload_kqv {
            opt.offload_kqv = Some(o);
        }
        if let Some(o) = self.op_offload {
            opt.op_offload = Some(o);
        }
        if let Some(b) = self.vram_buffer_gb {
            opt.custom_vram_buffer_gb = Some(b);
        }
        Ok(())
    }
}

/// llama.cpp cache type names -> the KV types Cluaiz supports.
fn parse_cache_type(name: &str) -> Result<KvCacheQuantization, String> {
    match name.to_lowercase().as_str() {
        "auto" => Ok(KvCacheQuantization::Auto),
        "f16" => Ok(KvCacheQuantization::Kv16),
        "q8_0" => Ok(KvCacheQuantization::Kv8),
        "q4_0" => Ok(KvCacheQuantization::Kv4),
        other => Err(format!("cache type '{}' is not supported: use auto, f16, q8_0 or q4_0", other)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveUserSettings {
    #[serde(default)]
    pub optimization: OptimizationControl,
    #[serde(default)]
    pub hardware_preset: String,
    #[serde(default)]
    pub telemetry_enabled: bool,
    #[serde(default = "default_cleanup_policy")]
    pub storage_cleanup_policy: String,
    /// User default samplers (llama.cpp names); a chat request's own values override these.
    #[serde(default)]
    pub samplers: SamplerDefaults,
    /// ONNX engine settings (speech, TTS, vision encoders)
    #[serde(default)]
    pub onnx: OnnxSettings,
}

/// ONNX Runtime session settings. Every field is read by the ONNX engine (engine/onnx).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OnnxSettings {
    /// -1 = GPU when available, 0 = CPU only
    #[serde(default = "OnnxSettings::default_n_gpu_layers")]
    pub n_gpu_layers: i32,
    /// 0 = ONNX Runtime decides
    #[serde(default)]
    pub intra_op_num_threads: usize,
    #[serde(default = "OnnxSettings::default_true")]
    pub enable_cpu_mem_arena: bool,
    /// 0 = no limit
    #[serde(default)]
    pub gpu_mem_limit_bytes: usize,
    #[serde(default = "OnnxSettings::default_arena_strategy")]
    pub arena_extend_strategy: String,
    #[serde(default)]
    pub use_deterministic_compute: bool,
}

impl OnnxSettings {
    fn default_n_gpu_layers() -> i32 { -1 }
    fn default_true() -> bool { true }
    fn default_arena_strategy() -> String { "kNextPowerOfTwo".to_string() }

    /// Applies a partial update (only the keys a client changed); unknown keys are an error.
    pub fn merged_with(&self, patch: &serde_json::Value) -> Result<Self, String> {
        let patch = patch.as_object().ok_or("ONNX settings update must be a JSON object")?;
        let mut merged = serde_json::to_value(self).map_err(|e| e.to_string())?;
        if let Some(fields) = merged.as_object_mut() {
            for (key, value) in patch {
                fields.insert(key.clone(), value.clone());
            }
        }
        serde_json::from_value(merged).map_err(|e| e.to_string())
    }
}

impl Default for OnnxSettings {
    fn default() -> Self {
        Self {
            n_gpu_layers: Self::default_n_gpu_layers(),
            intra_op_num_threads: 0,
            enable_cpu_mem_arena: true,
            gpu_mem_limit_bytes: 0,
            arena_extend_strategy: Self::default_arena_strategy(),
            use_deterministic_compute: false,
        }
    }
}

fn default_cleanup_policy() -> String {
    "Immediate".to_string()
}

impl Default for ActiveUserSettings {
    fn default() -> Self {
        Self {
            optimization: OptimizationControl::default(),
            hardware_preset: "Balanced".to_string(),
            telemetry_enabled: true,
            storage_cleanup_policy: default_cleanup_policy(),
            samplers: SamplerDefaults::default(),
            onnx: OnnxSettings::default(),
        }
    }
}

static CACHED_SETTINGS: Lazy<RwLock<ActiveUserSettings>> = Lazy::new(|| {
    let loaded = Settings::load_from_disk().unwrap_or_default();
    RwLock::new(loaded)
});

pub struct Settings;

impl Settings {
    /// Returns a copy of the current cached active user settings (0 disk reads).
    pub fn current() -> ActiveUserSettings {
        CACHED_SETTINGS.read().unwrap().clone()
    }

    /// Updates the cached settings and persists them to ~/.cluaiz/engine/config/settings.json
    pub fn update(new_settings: ActiveUserSettings) -> anyhow::Result<()> {
        let env = EnvironmentManager::current();
        env.ensure_config_dir()?;
        let path = env.config_dir().join("settings.json");
        let content = serde_json::to_string_pretty(&new_settings)?;
        std::fs::write(&path, content)?;
        *CACHED_SETTINGS.write().unwrap() = new_settings;
        Ok(())
    }

    /// Loads settings directly from disk or creates default if missing.
    pub fn load_from_disk() -> anyhow::Result<ActiveUserSettings> {
        let env = EnvironmentManager::current();
        let path = env.config_dir().join("settings.json");
        if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            let settings: ActiveUserSettings = serde_json::from_str(&data)?;
            Ok(settings)
        } else {
            Ok(ActiveUserSettings::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llama_n_ctx_round_trips_through_settings() {
        for (n_ctx, expected) in [(0, ContextSetting::Auto), (-1, ContextSetting::Max), (8192, ContextSetting::Custom(8192))] {
            let mut opt = OptimizationControl::default();
            opt.set_context_setting(ContextSetting::from_llama_n_ctx(n_ctx));
            assert_eq!(opt.context_setting(), expected);
            assert_eq!(opt.context_setting().to_llama_n_ctx(), n_ctx);
        }
    }

    #[test]
    fn load_fingerprint_ignores_samplers_but_tracks_load_settings() {
        let mut a = ActiveUserSettings::default();
        let base = a.optimization.load_fingerprint();
        a.samplers.temp = Some(0.1);
        assert_eq!(a.optimization.load_fingerprint(), base, "sampler change must not reload the model");
        a.optimization.kv_cache_quantization = KvCacheQuantization::Kv8;
        assert_ne!(a.optimization.load_fingerprint(), base, "KV type change must reload the model");
        let mut b = ActiveUserSettings::default();
        b.optimization.set_context_setting(ContextSetting::Custom(8192));
        assert_ne!(b.optimization.load_fingerprint(), base, "context change must reload the model");
    }

    #[test]
    fn thinking_budget_names_numbers_and_auto() {
        let parse = |v: serde_json::Value| serde_json::from_value::<ThinkingBudget>(v);
        assert_eq!(parse(serde_json::json!("low")).unwrap(), ThinkingBudget::Low);
        assert_eq!(parse(serde_json::json!("High")).unwrap(), ThinkingBudget::High);
        assert_eq!(parse(serde_json::json!(2500)).unwrap(), ThinkingBudget::Tokens(2500));
        assert_eq!(parse(serde_json::json!("2500")).unwrap(), ThinkingBudget::Tokens(2500));
        assert_eq!(parse(serde_json::json!(null)).unwrap(), ThinkingBudget::Auto);
        assert!(parse(serde_json::json!("sometimes")).is_err());
        assert!(parse(serde_json::json!(-5)).is_err());
        // round trip keeps the user's choice (names stay names, numbers stay numbers)
        assert_eq!(serde_json::to_value(ThinkingBudget::Medium).unwrap(), serde_json::json!("medium"));
        assert_eq!(serde_json::to_value(ThinkingBudget::Tokens(800)).unwrap(), serde_json::json!(800));
        // room = n_ctx - prompt (1000) - answer reserve (n_ctx / 4, 256..2048)
        let p = ThinkingPresets::default();
        let room = |n_ctx: usize| n_ctx - 1000 - (n_ctx / 4).clamp(256, 2048);
        let t = |b: ThinkingBudget, n_ctx: usize| b.limit(room(n_ctx), &p).tokens;
        use ThinkingBudget::*;
        for (n_ctx, low, medium, high, max) in [
            (4096, 518, 1036, 1554, 2072),
            (32768, 2048, 8192, 22290, 29720),
            (65536, 2048, 8192, 32768, 62488),
            (131072, 2048, 8192, 32768, 128024),
            (262144, 2048, 8192, 32768, 259096),
            (1048576, 2048, 8192, 32768, 1045528),
        ] {
            assert_eq!((t(Low, n_ctx), t(Medium, n_ctx), t(High, n_ctx), t(Max, n_ctx)), (low, medium, high, max), "n_ctx {}", n_ctx);
            assert_eq!(t(Auto, n_ctx), high, "auto = high at n_ctx {}", n_ctx);
        }
        // a user number bigger than the room is lowered so the answer still fits
        assert_eq!(Tokens(100_000).limit(2072, &p), ThinkingLimit { tokens: 2072, capped: true });
        assert_eq!(Tokens(500).limit(2072, &p), ThinkingLimit { tokens: 500, capped: false });
        // no room: no thinking, straight to the answer
        assert_eq!(High.limit(0, &p).tokens, 0);
        // default effort is saved with the settings and is not a reload reason
        let mut opt = OptimizationControl::default();
        let fp = opt.load_fingerprint();
        opt = opt.merged_with(&serde_json::json!({ "reasoning_effort": 3000 })).unwrap();
        assert_eq!(opt.reasoning_effort, ThinkingBudget::Tokens(3000));
        assert_eq!(opt.load_fingerprint(), fp);
    }

    #[test]
    fn onnx_settings_partial_update() {
        let saved = OnnxSettings::default();
        let updated = saved.merged_with(&serde_json::json!({ "n_gpu_layers": 0 })).unwrap();
        assert_eq!(updated.n_gpu_layers, 0);
        assert_eq!(updated.arena_extend_strategy, saved.arena_extend_strategy);
        assert!(saved.merged_with(&serde_json::json!({ "graph_optimization_level": "ORT_ENABLE_ALL" })).is_err());
        assert!(saved.merged_with(&serde_json::json!({ "n_gpu_layers": "many" })).is_err());
    }

    #[test]
    fn field_list_matches_the_struct() {
        let serialized = serde_json::to_value(OptimizationControl::default()).unwrap();
        let mut fields: Vec<&str> = serialized.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        let mut listed = OPTIMIZATION_FIELDS.to_vec();
        fields.sort_unstable();
        listed.sort_unstable();
        assert_eq!(fields, listed, "OPTIMIZATION_FIELDS must list every OptimizationControl field");
        // an old settings.json that still has removed keys loads (they are dropped on the next save)
        let old: OptimizationControl =
            serde_json::from_value(serde_json::json!({ "speculative_decoding": "On", "think_mode": "On" })).unwrap();
        assert_eq!(old.think_mode, FeatureState::On);
    }

    #[test]
    fn partial_update_keeps_other_settings() {
        let mut saved = OptimizationControl::default();
        saved.kv_cache_quantization = KvCacheQuantization::Kv8;
        saved.flash_attention = FeatureState::On;
        saved.custom_vram_buffer_gb = Some(0.5);

        // what the app sends: one key, lowercase value
        let updated = saved.merged_with(&serde_json::json!({ "think_mode": "on" })).unwrap();
        assert_eq!(updated.think_mode, FeatureState::On);
        assert_eq!(updated.kv_cache_quantization, KvCacheQuantization::Kv8, "other settings must stay");
        assert_eq!(updated.flash_attention, FeatureState::On);
        assert_eq!(updated.custom_vram_buffer_gb, Some(0.5));

        // a key given by its alias replaces the saved field
        let updated = saved.merged_with(&serde_json::json!({ "n_ctx": 8192 })).unwrap();
        assert_eq!(updated.context_setting(), ContextSetting::Custom(8192));

        // bad values and unknown or removed settings are an error, not silently ignored
        assert!(saved.merged_with(&serde_json::json!({ "think_mode": "sometimes" })).is_err());
        assert!(saved.merged_with(&serde_json::json!({ "speculative_decoding": "On" })).is_err());
        assert!(saved.merged_with(&serde_json::json!({ "flash_atention": "On" })).is_err());
        assert!(saved.merged_with(&serde_json::json!(["think_mode"])).is_err());
    }

    #[test]
    fn request_llama_params_override_settings() {
        let p: LlamaLoadParams = serde_json::from_str(
            r#"{"n_ctx": 8192, "ngl": 20, "cache_type_k": "q8_0", "cache_type_v": "q8_0", "flash_attn": "on", "n_threads": 6}"#,
        ).unwrap();
        let mut opt = OptimizationControl::default();
        p.apply_to(&mut opt).unwrap();
        assert_eq!(opt.context_setting(), ContextSetting::Custom(8192));
        assert_eq!(opt.n_gpu_layers, Some(20));
        assert_eq!(opt.kv_cache_quantization, KvCacheQuantization::Kv8);
        assert_eq!(opt.flash_attention, FeatureState::On);
        assert_eq!(opt.resolved_threads(8), (6, 6));
        assert!(opt.is_manual_placement());
        assert_ne!(opt.load_fingerprint(), OptimizationControl::default().load_fingerprint());

        // Missing fields keep auto; typos are rejected instead of silently ignored
        let mut auto = OptimizationControl::default();
        LlamaLoadParams::default().apply_to(&mut auto).unwrap();
        assert_eq!(auto, OptimizationControl::default());
        assert!(serde_json::from_str::<LlamaLoadParams>(r#"{"n_gpu_layer": 5}"#).is_err());
        let bad = LlamaLoadParams { cache_type_k: Some("q8_0".into()), cache_type_v: Some("f16".into()), ..Default::default() };
        assert!(bad.apply_to(&mut OptimizationControl::default()).is_err());
    }
}
