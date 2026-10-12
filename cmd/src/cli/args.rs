//! Flags shared by `run`, `bench` and `show`, named like llama.cpp's own tools (llama-cli, llama-bench).
//! Load flags become the same `LlamaLoadParams` an API request's `"llama"` object becomes, so the CLI and
//! the API load a model the same way.

use cluaiz_core::{FeatureState, LlamaLoadParams, OptimizationControl, SamplerDefaults, ThinkingBudget};

/// llama.cpp short flags longer than one letter (clap only knows one-letter short flags).
const LLAMA_SHORT_FLAGS: &[(&str, &str)] = &[
    ("-ngl", "--n-gpu-layers"),
    ("-ncmoe", "--n-cpu-moe"),
    ("-ctk", "--cache-type-k"),
    ("-ctv", "--cache-type-v"),
    ("-fa", "--flash-attn"),
    ("-ub", "--ubatch-size"),
    ("-tb", "--threads-batch"),
    ("-nkvo", "--no-kv-offload"),
    ("-sys", "--system-prompt"),
];

/// Rewrites llama.cpp's multi-letter short flags (`-ngl 20`, `-fa=on`) to their long names.
/// Nothing after `--` is touched.
pub fn normalize_llama_flags(args: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut passthrough = false;
    for arg in args {
        if passthrough || arg == "--" {
            passthrough = true;
            out.push(arg);
            continue;
        }
        let (flag, value) = match arg.split_once('=') {
            Some((f, v)) => (f, Some(v)),
            None => (arg.as_str(), None),
        };
        match LLAMA_SHORT_FLAGS.iter().find(|(short, _)| *short == flag) {
            Some((_, long)) => out.push(match value {
                Some(v) => format!("{}={}", long, v),
                None => long.to_string(),
            }),
            None => out.push(arg),
        }
    }
    out
}

/// `-ngl` value: llama.cpp accepts a number, "all" or "auto".
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GpuLayers {
    Auto,
    All,
    Count(i32),
}

fn parse_gpu_layers(v: &str) -> Result<GpuLayers, String> {
    match v.to_lowercase().as_str() {
        "auto" => Ok(GpuLayers::Auto),
        "all" | "-1" => Ok(GpuLayers::All),
        n => n.parse::<i32>().ok().filter(|n| *n >= 0).map(GpuLayers::Count)
            .ok_or_else(|| format!("'{}': use a number of layers, all or auto", v)),
    }
}

/// `-c` value: "auto" (planner), "max" (model's limit) or a number of tokens; as llama.cpp n_ctx 0 / -1 / N.
fn parse_ctx(v: &str) -> Result<i64, String> {
    match v.to_lowercase().as_str() {
        "auto" => Ok(0),
        "max" => Ok(-1),
        n => n.parse::<i64>().ok().filter(|n| *n > 0)
            .ok_or_else(|| format!("'{}': use a number of tokens, auto or max", v)),
    }
}

fn parse_feature(v: &str) -> Result<FeatureState, String> {
    match v.to_lowercase().as_str() {
        "on" | "true" | "1" => Ok(FeatureState::On),
        "off" | "false" | "0" => Ok(FeatureState::Off),
        "auto" => Ok(FeatureState::Auto),
        _ => Err(format!("'{}': use on, off or auto", v)),
    }
}

/// llama.cpp `--reasoning-budget`: -1 = unrestricted, 0 = no thinking, N = at most N tokens.
fn parse_reasoning_budget(v: &str) -> Result<ThinkingBudget, String> {
    match v.trim().parse::<i64>() {
        Ok(-1) => Ok(ThinkingBudget::Max),
        Ok(n) if n >= 0 => u32::try_from(n).map(ThinkingBudget::Tokens).map_err(|e| e.to_string()),
        _ => Err(format!("'{}': use -1 (unrestricted), 0 (no thinking) or a number of tokens", v)),
    }
}

/// How the model is loaded. Every flag is optional: a missing one keeps settings.json / the planner.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct LoadArgs {
    /// Context size in tokens: a number, auto (planner) or max (the model's limit)
    #[arg(short = 'c', long = "ctx-size", value_name = "N|auto|max", value_parser = parse_ctx, help_heading = "Load")]
    pub ctx_size: Option<i64>,
    /// Layers on the GPU (-ngl): a number, all, or auto (planner)
    #[arg(long = "n-gpu-layers", visible_alias = "gpu-layers", value_name = "N|all|auto", value_parser = parse_gpu_layers, help_heading = "Load")]
    pub n_gpu_layers: Option<GpuLayers>,
    /// Keep the MoE experts of the first N layers in RAM (-ncmoe)
    #[arg(long = "n-cpu-moe", value_name = "N", help_heading = "Load")]
    pub n_cpu_moe: Option<u32>,
    /// KV cache type for K (-ctk): f16, q8_0, q4_0 or auto
    #[arg(long = "cache-type-k", value_name = "TYPE", help_heading = "Load")]
    pub cache_type_k: Option<String>,
    /// KV cache type for V (-ctv): f16, q8_0, q4_0 or auto
    #[arg(long = "cache-type-v", value_name = "TYPE", help_heading = "Load")]
    pub cache_type_v: Option<String>,
    /// Flash attention (-fa): on, off or auto
    #[arg(long = "flash-attn", value_name = "on|off|auto", help_heading = "Load")]
    pub flash_attn: Option<String>,
    /// Logical batch size for prompt processing
    #[arg(short = 'b', long = "batch-size", value_name = "N", help_heading = "Load")]
    pub batch_size: Option<u32>,
    /// Physical batch size (-ub)
    #[arg(long = "ubatch-size", value_name = "N", help_heading = "Load")]
    pub ubatch_size: Option<u32>,
    /// CPU threads for generation
    #[arg(short = 't', long = "threads", value_name = "N", help_heading = "Load")]
    pub threads: Option<i32>,
    /// CPU threads for prompt processing (-tb)
    #[arg(long = "threads-batch", value_name = "N", help_heading = "Load")]
    pub threads_batch: Option<i32>,
    /// Memory-map the model file
    #[arg(long = "mmap", overrides_with = "no_mmap", help_heading = "Load")]
    pub mmap: bool,
    /// Read the model into RAM instead of memory-mapping it
    #[arg(long = "no-mmap", overrides_with = "mmap", help_heading = "Load")]
    pub no_mmap: bool,
    /// Keep the whole KV cache in RAM (-nkvo)
    #[arg(long = "no-kv-offload", help_heading = "Load")]
    pub no_kv_offload: bool,
    /// Do not compute large batches of RAM-held weights on the GPU
    #[arg(long = "no-op-offload", help_heading = "Load")]
    pub no_op_offload: bool,
    /// VRAM to keep free, in GB (default: the planner's rule)
    #[arg(long = "vram-buffer", value_name = "GB", help_heading = "Load")]
    pub vram_buffer: Option<f64>,
}

impl LoadArgs {
    /// Writes the given flags over the user's settings, exactly like an API request's `"llama"` object.
    pub fn apply(&self, opt: &mut OptimizationControl) -> Result<(), String> {
        let params = LlamaLoadParams {
            n_ctx: self.ctx_size,
            n_gpu_layers: match self.n_gpu_layers {
                Some(GpuLayers::All) => Some(-1),
                Some(GpuLayers::Count(n)) => Some(n),
                Some(GpuLayers::Auto) | None => None,
            },
            n_cpu_moe: self.n_cpu_moe,
            cache_type_k: self.cache_type_k.clone(),
            cache_type_v: self.cache_type_v.clone(),
            flash_attn: self.flash_attn.clone().map(serde_json::Value::String),
            n_batch: self.batch_size,
            n_ubatch: self.ubatch_size,
            n_threads: self.threads,
            n_threads_batch: self.threads_batch,
            use_mmap: match (self.mmap, self.no_mmap) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            },
            offload_kqv: self.no_kv_offload.then_some(false),
            op_offload: self.no_op_offload.then_some(false),
            vram_buffer_gb: self.vram_buffer,
        };
        params.apply_to(opt)?;
        if self.n_gpu_layers == Some(GpuLayers::Auto) {
            opt.n_gpu_layers = None;
        }
        Ok(())
    }
}

/// Sampler values for this run (llama.cpp names). A missing one uses settings.json, then the model's default.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct SampleArgs {
    /// Temperature
    #[arg(long, value_name = "N", help_heading = "Sampling")]
    pub temp: Option<f32>,
    /// Top-k sampling (0 = off)
    #[arg(long = "top-k", value_name = "N", help_heading = "Sampling")]
    pub top_k: Option<i32>,
    /// Top-p (nucleus) sampling
    #[arg(long = "top-p", value_name = "N", help_heading = "Sampling")]
    pub top_p: Option<f32>,
    /// Min-p sampling
    #[arg(long = "min-p", value_name = "N", help_heading = "Sampling")]
    pub min_p: Option<f32>,
    /// Penalty for repeated tokens
    #[arg(long = "repeat-penalty", value_name = "N", help_heading = "Sampling")]
    pub repeat_penalty: Option<f32>,
    /// Presence penalty
    #[arg(long = "presence-penalty", value_name = "N", help_heading = "Sampling")]
    pub presence_penalty: Option<f32>,
    /// Frequency penalty
    #[arg(long = "frequency-penalty", value_name = "N", help_heading = "Sampling")]
    pub frequency_penalty: Option<f32>,
    /// Random seed
    #[arg(short = 's', long, value_name = "N", help_heading = "Sampling")]
    pub seed: Option<u32>,
}

impl SampleArgs {
    pub fn samplers(&self) -> SamplerDefaults {
        SamplerDefaults {
            temp: self.temp,
            top_p: self.top_p,
            top_k: self.top_k,
            min_p: self.min_p,
            presence_penalty: self.presence_penalty,
            frequency_penalty: self.frequency_penalty,
            repeat_penalty: self.repeat_penalty,
            seed: self.seed,
        }
    }
}

/// Thinking of reasoning models. Missing flags use settings.json.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct ThinkArgs {
    /// Thinking: on, off or auto (the model decides)
    #[arg(long, value_name = "on|off|auto", value_parser = parse_feature, help_heading = "Thinking")]
    pub think: Option<FeatureState>,
    /// Thinking budget: auto, low, medium, high, max or a number of tokens
    #[arg(long = "reasoning-effort", value_name = "LEVEL|N", value_parser = ThinkingBudget::parse, help_heading = "Thinking")]
    pub reasoning_effort: Option<ThinkingBudget>,
    /// llama.cpp form of the budget: -1 = unrestricted, 0 = no thinking, N = tokens
    #[arg(long = "reasoning-budget", value_name = "N", value_parser = parse_reasoning_budget, conflicts_with = "reasoning_effort", allow_hyphen_values = true, help_heading = "Thinking")]
    pub reasoning_budget: Option<ThinkingBudget>,
    /// Text written before the end of thinking when the budget runs out
    #[arg(long = "reasoning-budget-message", value_name = "TEXT", help_heading = "Thinking")]
    pub reasoning_budget_message: Option<String>,
    /// Do not print the model's thinking (it still thinks)
    #[arg(long = "hide-thinking", help_heading = "Thinking")]
    pub hide_thinking: bool,
}

impl ThinkArgs {
    pub fn effort(&self) -> Option<ThinkingBudget> {
        self.reasoning_effort.or(self.reasoning_budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llama_short_flags_become_long_names() {
        let args = ["run", "m", "-ngl", "20", "-fa=on", "-c", "4096", "--", "-ngl"].map(String::from);
        assert_eq!(
            normalize_llama_flags(args),
            ["run", "m", "--n-gpu-layers", "20", "--flash-attn=on", "-c", "4096", "--", "-ngl"].map(String::from)
        );
    }

    #[test]
    fn load_flags_apply_like_llama_params() {
        let mut opt = OptimizationControl::default();
        opt.n_gpu_layers = Some(10);
        let args = LoadArgs { ctx_size: Some(8192), n_gpu_layers: Some(GpuLayers::Auto), no_mmap: true, ..Default::default() };
        args.apply(&mut opt).unwrap();
        assert_eq!(opt.n_gpu_layers, None);
        assert_eq!(opt.custom_context_window, Some(8192));
        assert_eq!(opt.use_mmap, Some(false));
        let bad = LoadArgs { cache_type_k: Some("q5_1".into()), ..Default::default() };
        assert!(bad.apply(&mut opt).is_err());
    }

    #[test]
    fn value_parsers() {
        assert_eq!(parse_gpu_layers("all"), Ok(GpuLayers::All));
        assert_eq!(parse_gpu_layers("99"), Ok(GpuLayers::Count(99)));
        assert!(parse_gpu_layers("-3").is_err());
        assert_eq!(parse_ctx("max"), Ok(-1));
        assert!(parse_ctx("0").is_err());
        assert_eq!(parse_reasoning_budget("-1"), Ok(ThinkingBudget::Max));
        assert_eq!(parse_reasoning_budget("0"), Ok(ThinkingBudget::Tokens(0)));
    }
}
