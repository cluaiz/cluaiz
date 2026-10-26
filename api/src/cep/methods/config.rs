//! `config.*`: the saved settings, by section (`gguf`, `onnx`, `optimization`, `context`,
//! `storage`, `kv`, `engine`).
//!
//! `config.set` changes only the fields it is given and answers with the section as saved.

use crate::cep::CallCtx;
use crate::handlers::chat::context_plan;
use cluaiz_cep::{Call, CepError, Meta, Registry, Scope};
use cluaiz_core::Settings;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Section {
    /// llama.cpp loading and the default samplers
    Gguf,
    /// ONNX Runtime: execution provider, device, threads
    Onnx,
    /// Context window, KV cache type, mmap, think mode…
    Optimization,
    /// The context runtime planner's policy
    Context,
    /// Temporary media cleanup
    Storage,
    /// Saved KV cache per chat: enabled, disk and age limits
    Kv,
    /// permission.json: model slots, API port and host, authentication, agent security mode
    Engine,
}

const SECTIONS: [Section; 7] = [Section::Gguf, Section::Onnx, Section::Optimization, Section::Context, Section::Storage, Section::Kv, Section::Engine];

impl Section {
    fn name(self) -> &'static str {
        match self {
            Section::Gguf => "gguf",
            Section::Onnx => "onnx",
            Section::Optimization => "optimization",
            Section::Context => "context",
            Section::Storage => "storage",
            Section::Kv => "kv",
            Section::Engine => "engine",
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Get {
    /// One section; all of them when absent
    #[serde(default)]
    section: Option<Section>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Set {
    /// The section to change
    section: Section,
    /// Only the fields to change
    patch: Value,
}

/// GGUF page. llama.cpp meanings: n_gpu_layers -1 = Auto, n_ctx 0 = Auto / -1 = Max.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct GgufPatch {
    hardware_and_execution: Option<GgufHardware>,
    samplers: Option<GgufSamplers>,
    user_moved_flags: Option<GgufFlags>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct GgufHardware {
    n_gpu_layers: Option<i32>,
    n_ctx: Option<i64>,
    no_mmap: Option<bool>,
    batch_size: Option<u32>,
    ubatch_size: Option<u32>,
}

/// Default samplers; null = the model's own default (then llama.cpp's)
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct GgufSamplers {
    temp: Option<f32>,
    top_p: Option<f32>,
    top_k: Option<i32>,
    min_p: Option<f32>,
    presence_penalty: Option<f32>,
    frequency_penalty: Option<f32>,
    repeat_penalty: Option<f32>,
    seed: Option<u32>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct GgufFlags {
    think_mode: Option<cluaiz_core::FeatureState>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoragePatch {
    cleanup_policy: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KvPatch {
    enabled: Option<bool>,
    max_disk_gb: Option<f64>,
    max_age_days: Option<u32>,
}

fn bad_patch(section: Section, e: impl std::fmt::Display) -> CepError {
    CepError::invalid_params(format!("Invalid {} setting: {e}", section.name()), vec![json!({ "path": "patch", "message": e.to_string() })], None)
}

fn save_error(e: impl std::fmt::Display) -> CepError {
    CepError::unavailable("settings_not_saved", format!("Settings could not be saved: {e}"))
}

/// The section as saved. For gguf: the saved values, null = automatic (not the resolved defaults,
/// so saving the page back never turns "automatic" into fixed numbers).
async fn section_value(ctx: &CallCtx, section: Section) -> Value {
    let settings = Settings::current();
    match section {
        Section::Gguf => {
            let opt = &settings.optimization;
            json!({
                "hardware_and_execution": {
                    "n_gpu_layers": opt.n_gpu_layers.unwrap_or(-1),
                    "n_ctx": opt.context_setting().to_llama_n_ctx(),
                    "no_mmap": opt.use_mmap.map(|m| !m),
                    "batch_size": opt.batch_size,
                    "ubatch_size": opt.ubatch_size
                },
                "samplers": settings.samplers,
                "user_moved_flags": { "think_mode": opt.think_mode }
            })
        }
        Section::Onnx => json!(settings.onnx),
        Section::Optimization => json!(settings.optimization),
        Section::Context => json!(settings.context_runtime),
        Section::Storage => json!({ "cleanup_policy": settings.storage_cleanup_policy }),
        Section::Kv => json!(settings.chat_memory),
        Section::Engine => json!(clear_missing_slots(ctx).await),
    }
}

/// Model slots that point at a model no longer installed (deleted by hand or unregistered) are
/// emptied and saved, so the settings never name a model that is gone
async fn clear_missing_slots(ctx: &CallCtx) -> cluaiz_security::PermissionSchema {
    let installed = cluaiz_models::registry::InstalledStateRegistry::load().installed_models;
    let mut schema = ctx.state.permission_schema.write().await;
    let mut changed = false;
    for slot in schema.active_slots.values_mut() {
        if slot.model_id.as_ref().is_some_and(|id| !installed.contains_key(id)) {
            *slot = cluaiz_security::SlotConfig::default();
            changed = true;
        }
    }
    if changed {
        let _ = schema.save();
    }
    schema.clone()
}

fn set_gguf(patch: Value) -> Result<(), CepError> {
    let parsed: GgufPatch = serde_json::from_value(patch).map_err(|e| bad_patch(Section::Gguf, e))?;
    let mut current = Settings::current();
    if let Some(hw) = parsed.hardware_and_execution {
        let opt = &mut current.optimization;
        if let Some(n) = hw.n_gpu_layers {
            // -1 = Auto (planner decides), 0 = CPU only, N = exactly N layers
            opt.n_gpu_layers = if n < 0 { None } else { Some(n) };
        }
        if let Some(n_ctx) = hw.n_ctx {
            opt.set_context_setting(cluaiz_core::ContextSetting::from_llama_n_ctx(n_ctx));
        }
        if let Some(no_mmap) = hw.no_mmap {
            opt.use_mmap = Some(!no_mmap);
        }
        if let Some(b) = hw.batch_size {
            opt.batch_size = Some(b);
        }
        if let Some(ub) = hw.ubatch_size {
            opt.ubatch_size = Some(ub);
        }
    }
    if let Some(sp) = parsed.samplers {
        current.samplers = cluaiz_core::SamplerDefaults {
            temp: sp.temp,
            top_p: sp.top_p,
            top_k: sp.top_k,
            min_p: sp.min_p,
            presence_penalty: sp.presence_penalty,
            frequency_penalty: sp.frequency_penalty,
            repeat_penalty: sp.repeat_penalty,
            seed: sp.seed,
        };
    }
    if let Some(think_mode) = parsed.user_moved_flags.and_then(|f| f.think_mode) {
        current.optimization.think_mode = think_mode;
    }
    Settings::update(current).map_err(save_error)
}

/// The first write of the context section backs up `settings.json` next to it
fn set_context(patch: Value) -> Result<(), CepError> {
    let Some(fields) = patch.as_object() else {
        return Err(bad_patch(Section::Context, "the patch must be an object"));
    };
    let mut settings = Settings::current();
    let mut value = serde_json::to_value(&settings.context_runtime).unwrap_or_default();
    for (key, field) in fields {
        if value.get(key).is_none() {
            return Err(bad_patch(Section::Context, format!("unknown field '{key}'")));
        }
        value[key] = field.clone();
    }
    let policy: cluaiz_context::Policy = serde_json::from_value(value)
        .ok()
        .filter(context_plan::valid)
        .ok_or_else(|| bad_patch(Section::Context, "percentages must be whole numbers from 0 to 100 and recent_turns_min at most 50"))?;
    let path = cluaiz_core::EnvironmentManager::current().config_dir().join("settings.json");
    let has_section = std::fs::read_to_string(&path).is_ok_and(|text| text.contains("\"context_runtime\""));
    if path.exists() && !has_section {
        let backup = path.with_file_name(format!("settings.json.bak-{}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
        std::fs::copy(&path, &backup).map_err(|e| save_error(format!("could not back up settings.json: {e}")))?;
        tracing::info!("🗂️ [Context] Backed up settings.json to {:?}", backup);
    }
    settings.context_runtime = policy;
    Settings::update(settings).map_err(save_error)
}

/// The limits apply at once
fn set_kv(patch: Value) -> Result<(), CepError> {
    let patch: KvPatch = serde_json::from_value(patch).map_err(|e| bad_patch(Section::Kv, e))?;
    let mut settings = Settings::current();
    if let Some(enabled) = patch.enabled {
        settings.chat_memory.enabled = enabled;
    }
    if let Some(gb) = patch.max_disk_gb {
        if !gb.is_finite() || gb < 0.0 {
            return Err(bad_patch(Section::Kv, "max_disk_gb must be 0 or more"));
        }
        settings.chat_memory.max_disk_gb = gb;
    }
    if let Some(days) = patch.max_age_days {
        settings.chat_memory.max_age_days = days;
    }
    let limits = settings.chat_memory.clone();
    Settings::update(settings).map_err(save_error)?;
    cluaiz_runtime::chat_memory::enforce_limits(&limits, None);
    Ok(())
}

async fn set(ctx: &CallCtx, section: Section, patch: Value) -> Result<(), CepError> {
    match section {
        Section::Gguf => tokio::task::spawn_blocking(move || set_gguf(patch)).await.map_err(|_| CepError::internal("config.set"))?,
        Section::Onnx => {
            let mut current = Settings::current();
            current.onnx = current.onnx.merged_with(&patch).map_err(|e| bad_patch(section, e))?;
            Settings::update(current).map_err(save_error)
        }
        Section::Optimization => {
            // Merged over the saved settings: replacing the whole struct would reset every other field
            let mut current = Settings::current();
            current.optimization = current.optimization.merged_with(&patch).map_err(|e| bad_patch(section, e))?;
            Settings::update(current).map_err(save_error)
        }
        Section::Context => set_context(patch),
        Section::Storage => {
            let patch: StoragePatch = serde_json::from_value(patch).map_err(|e| bad_patch(section, e))?;
            let mut current = Settings::current();
            current.storage_cleanup_policy = patch.cleanup_policy;
            Settings::update(current).map_err(save_error)
        }
        Section::Kv => set_kv(patch),
        Section::Engine => {
            let mut schema = ctx.state.permission_schema.read().await.clone();
            schema.merge_patch(&patch);
            schema.save().map_err(save_error)?;
            *ctx.state.permission_schema.write().await = schema;
            Ok(())
        }
    }
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry
        .add(
            "config.get",
            Meta::new(Scope::Read, "The saved settings: one section, or all of them").no_mcp().example(
                json!({ "section": "kv" }),
                json!({ "kv": { "enabled": true, "max_disk_gb": 10.0, "max_age_days": 30 } }),
            ),
            |ctx: CallCtx, _: Call, p: Get| async move {
                let sections: Vec<Section> = p.section.map(|s| vec![s]).unwrap_or_else(|| SECTIONS.to_vec());
                let mut out = serde_json::Map::new();
                for section in sections {
                    out.insert(section.name().to_string(), section_value(&ctx, section).await);
                }
                Ok(Value::Object(out))
            },
        )
        .add(
            "config.set",
            Meta::new(Scope::Write, "Changes the given fields of one settings section; answers with the section as saved")
                .no_mcp()
                .error(-32602, "invalid_params", "An unknown field or an invalid value in `patch` (section engine ignores unknown fields instead)")
                .error(-32010, "settings_not_saved", "settings.json or permission.json could not be written")
                .example(
                json!({ "section": "context", "patch": { "cross_chat": "ask" } }),
                json!({ "context": { "cross_chat": "ask" } }),
            ),
            |ctx: CallCtx, _: Call, p: Set| async move {
                set(&ctx, p.section, p.patch).await?;
                let mut out = serde_json::Map::new();
                out.insert(p.section.name().to_string(), section_value(&ctx, p.section).await);
                Ok(Value::Object(out))
            },
        );
}
