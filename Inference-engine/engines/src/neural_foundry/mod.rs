// cluaiz-engine: Core Foundry - The cluaiz Engine Core
// Final integration of Registry, Intelligence, Runtime, and Security.

pub mod registry;
pub mod intelligence;
pub mod runtime;
pub mod security;
pub mod ingestion;
pub mod executor;

use registry::SkillRegistry;
use intelligence::skill_router::SkillRouter;
use runtime::wasm_host::WasmHost;
use runtime::mcp_gateway::McpGateway;
use security::guard::{PermissionGuard, PermissionLevel};
use tracing::{info, warn};
use cluaiz_shared::hardware::memory::kv_cache::stitching::cluaizSignal;
use neural_core::interfaces::memory_contract::MappedBuffer;
use std::sync::{Mutex, Arc};
use std::path::PathBuf;

// Removed fixed MAX_ACTIVE_SKILLS limit. Bounding is now purely dynamic based on hardware capacity.
// Constant threshold for the ONNX semantic similarity match.

pub struct IntentResult {
    pub responses: Vec<String>,
    pub signals: Vec<cluaizSignal>,
    pub missing_caches: Vec<(PathBuf, String)>, // (kv_cache_path, skill_content)
}

pub struct CoreFoundry {
    pub registry: SkillRegistry,
    pub router: SkillRouter,
    pub wasm_runtime: WasmHost,
    pub mcp_gateway: McpGateway,
    pub guard: PermissionGuard,
    pub active_skill_ids: Mutex<Vec<String>>,
}

impl Default for CoreFoundry {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreFoundry {
    pub fn new() -> Self {
        Self {
            registry: SkillRegistry::new(),
            router: SkillRouter::new(),
            wasm_runtime: WasmHost::new(),
            mcp_gateway: McpGateway::new(),
            guard: PermissionGuard::new(),
            active_skill_ids: Mutex::new(Vec::new()),
        }
    }

    pub fn initialize(&mut self, _skills_dir: &str) {
        let env = cluaiz_shared::environment::EnvironmentManager::current();
        cluaiz_shared::dev_info!("[cluaiz] Initializing Core Foundry from: {}", env.skills_dir().display());
        for dir in [env.skills_dir(), env.extensions_dir(), env.plugins_dir(), env.mcp_dir()] {
            self.registry.load_from_directory(&dir.to_string_lossy());
        }
    }

    /// The cluaiz Flow: Prompt -> Multi-Route -> Execute
    pub async fn process_intent(&self, prompt: &str, pre_matched_skills: Option<Vec<String>>) -> anyhow::Result<IntentResult> {
        let skill_ids = pre_matched_skills.unwrap_or_else(|| self.router.match_intent(prompt, &self.registry));
        let mut result = IntentResult { responses: Vec::new(), signals: Vec::new(), missing_caches: Vec::new() };

        if skill_ids.is_empty() {
            return Ok(result);
        }

        info!("🧪 [CoreFoundry] Multi-Skill Fusion Active: {} skills detected.", skill_ids.len());

        for (i, skill_id) in skill_ids.iter().enumerate() {
            // Note: Bounding is no longer hardcoded by active skills count limit, 
            // but is bounded below strictly by the dynamic RAM capacity pulse lock.

            // 1. Fetch skill from registry
            let skill = match self.registry.skills.iter().find(|s| &s.manifest.id == skill_id) {
                Some(s) => s,
                None => continue,
            };
            
            // 2. Dynamic Memory Management (RAM/VRAM Bounding)
            {
                // Fetch real-time hardware telemetry to ensure we don't cause OOM.
                let pulse = cluaiz_shared::hardware::telemetry::get_pulse();
                let pulse_lock = pulse.pulse.read().unwrap();
                let used_mb = pulse_lock.ram.used_gb * 1024.0;
                let util = pulse_lock.ram.utilization_pct as f64;
                let available_ram_mb = if util > 0.1 {
                    (used_mb / (util / 100.0)) - used_mb
                } else {
                    8192.0 // Fallback to 8GB free if telemetry is spinning up
                };
                
                // Estimate skill size (mock 50MB per skill KV cache for architecture demonstration)
                let skill_est_size_mb = 50.0; 

                let mut active_ids = self.active_skill_ids.lock().unwrap();
                
                // If adding this skill exceeds safe bounds, perform LRU eviction dynamically
                while (active_ids.len() as f32 + 1.0) * skill_est_size_mb >= available_ram_mb as f32 * 0.8 {
                    if !active_ids.is_empty() {
                        let evicted_id = active_ids.remove(0);
                        cluaiz_shared::dev_info!("[cluaiz] [VRAM] Bounding limit hit. Evicting LRU skill: {}", evicted_id);
                    } else {
                        break;
                    }
                }

                active_ids.retain(|id| id != skill_id);
                active_ids.push(skill_id.to_string());
            }

            // 3. Map cluaiz Signal (Zero-Copy Dual-Cache)
            // Offload the blocking disk I/O to a background thread to prevent blocking the async runtime
            let skill_id_clone = skill_id.clone();
            let skill_path_clone = skill.path.clone();
            let skill_manifest_clone = skill.manifest.clone();

            pub enum SkillLoadResult {
                Signal {
                    raw_data: cluaiz_shared::hardware::memory::buffer::SafeTensorsMappedBuffer,
                    token_count: usize,
                    head_dim: usize,
                },
                MissingCache {
                    kv_cache_path: PathBuf,
                    content: String,
                },
                TextPayload {
                    content: String,
                },
                NoModel,
                None,
            }

            let load_result = tokio::task::spawn_blocking(move || {
                let permissions = crate::neural_foundry::security::permission_schema::PermissionSchema::load();
                
                if let Some(gen_model) = permissions.get_active_chat_model() {
                    let gen_model_safe = gen_model.replace(":", "-");
                    let cache_dir = skill_path_clone.join(".cache");
                    let kv_cache_path = cache_dir.join(format!("{}.kvcache.safetensors", gen_model_safe));
                    
                    if !permissions.enable_kvcache {
                        let content = extract_skill_body(&skill_path_clone)
                            .unwrap_or_else(|| skill_manifest_clone.description.clone());
                        return SkillLoadResult::TextPayload { content };
                    }
                    
                    let mut cache_exists = kv_cache_path.exists();
                    let mut layers = None;
                    let mut kv_heads = None;

                    if cache_exists {
                        let roster = crate::models::registry::CoreRoster::load_roster();
                        if let Some(manifest) = roster.iter().find(|m| m.id == gen_model) {
                            if let Some(local_path) = &manifest.local_path {
                                let dna_path = std::path::Path::new(local_path).join("structural_dna.json");
                                if let Ok(dna_content) = std::fs::read_to_string(&dna_path) {
                                    if let Ok(dna) = serde_json::from_str::<cluaiz_shared::StructuralDNA>(&dna_content) {
                                        layers = dna.layer_count;
                                        kv_heads = dna.attention_head_count_kv.or(dna.attention_head_count);
                                    }
                                }
                            }
                        }

                        let mut is_valid = true;
                        if let Ok(metadata) = std::fs::metadata(&kv_cache_path) {
                            if metadata.len() == 0 {
                                is_valid = false;
                            }
                            
                            // Timestamp Invalidation Check
                            let skill_md_path = skill_path_clone.join("SKILL.md");
                            if let Ok(skill_md_meta) = std::fs::metadata(&skill_md_path) {
                                if let (Ok(cache_time), Ok(skill_time)) = (metadata.modified(), skill_md_meta.modified()) {
                                    if skill_time > cache_time {
                                        tracing::warn!("⚠️ [CoreFoundry] SKILL.md for {} was modified. Invalidating stale cache.", skill_id_clone);
                                        is_valid = false;
                                    }
                                }
                            }
                        } else {
                            is_valid = false;
                        }

                        if !is_valid {
                            let _ = std::fs::remove_file(&kv_cache_path);
                            cache_exists = false;
                        }
                    }
                    
                    if cache_exists {
                        use cluaiz_shared::hardware::memory::buffer::SafeTensorsMappedBuffer;
                        
                        if let Ok(mapped_buffer) = SafeTensorsMappedBuffer::from_file(&kv_cache_path) {
                            return SkillLoadResult::Signal {
                                raw_data: mapped_buffer,
                                token_count: skill_manifest_clone.Core_metadata.as_ref().map_or(0, |m| m.token_count),
                                head_dim: skill_manifest_clone.Core_metadata.as_ref().map_or(0, |m| m.head_dim),
                            };
                        }
                    } else {
                        tracing::warn!("⚠️ [CoreFoundry] {} missing for skill {}. Flagging for Sovereign Compiler.", kv_cache_path.display(), skill_id_clone);
                        
                        let content = extract_skill_body(&skill_path_clone)
                            .unwrap_or_else(|| skill_manifest_clone.description.clone());
                        
                        return SkillLoadResult::MissingCache { kv_cache_path, content };
                    }
                } else {
                    return SkillLoadResult::NoModel;
                }
                SkillLoadResult::None
            }).await?;

            match load_result {
                SkillLoadResult::Signal { raw_data, token_count, head_dim } => {
                    result.signals.push(cluaizSignal {
                        raw_data: Arc::new(raw_data),
                        token_count,
                        head_dim,
                    });
                }
                SkillLoadResult::MissingCache { kv_cache_path, content } => {
                    result.missing_caches.push((kv_cache_path, content));
                }
                SkillLoadResult::TextPayload { content } => {
                    result.responses.push(content);
                }
                SkillLoadResult::NoModel => {
                    warn!("⚠️ [CoreFoundry] No text model assigned in Permission.json. Skipping Zero-Copy injection for skill {}.", skill_id);
                }
                SkillLoadResult::None => {}
            }
            
            // WASM logic execution is handled during streaming/generation interceptor.
            self.guard.validate_action(&skill.manifest, PermissionLevel::ReadOnly)?;
        }
        
        Ok(result)
    }
}

pub fn extract_skill_body(skill_dir: &std::path::Path) -> Option<String> {
    // 🧠 1. ZERO-LATENCY FFI BRAIN INJECTION
    if let Some(skill_name) = skill_dir.file_name().map(|s| s.to_string_lossy().to_string()) {
        let bridge = crate::memory::storage_bridge::load_storage_bridge();
        if let Some(raw_bytes) = bridge.inject_context(&skill_name) {
            if let Ok(content) = String::from_utf8(raw_bytes) {
                return Some(content);
            }
        }
    }

    // 🏢 2. DYNAMIC TOOL DISCOVERY & ADAPTATION
    // We read SKILL.md for the user's instructions, and then we read the manifest
    // to dynamically append the `cel_grammar` trigger instruction, so the Engine automatically adapts.
    let mut final_context = String::new();

    let skill_md_path = skill_dir.join("SKILL.md");
    if skill_md_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&skill_md_path) {
            let normalized = content.replace("\r\n", "\n");
            let lines: Vec<&str> = normalized.lines().collect();
            
            let mut first_line_idx = None;
            for (i, line) in lines.iter().enumerate() {
                if !line.trim().is_empty() {
                    if line.trim() == "---" {
                        first_line_idx = Some(i);
                        break;
                    }
                }
            }

            if let Some(start_idx) = first_line_idx {
                let mut closing_idx = None;
                for i in (start_idx + 1)..lines.len() {
                    if lines[i].trim() == "---" {
                        closing_idx = Some(i);
                        break;
                    }
                }

                if let Some(end_idx) = closing_idx {
                    let body_lines = &lines[end_idx + 1..];
                    let body = body_lines.join("\n").trim().to_string();
                    if !body.is_empty() {
                        final_context.push_str(&body);
                    }
                }
            } else {
                final_context.push_str(content.trim());
            }
        }
    }

    let ext_yaml_path = skill_dir.join("manifest-extension.yaml");
    let plugin_yaml_path = skill_dir.join("manifest-plugin.yaml");
    let mcp_yaml_path = skill_dir.join("manifest-mcp.yaml");

    let skill_name = skill_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

    for (yaml_path, tool_type) in &[(&ext_yaml_path, "extension"), (&plugin_yaml_path, "plugin"), (&mcp_yaml_path, "mcp")] {
        if yaml_path.exists() {
            if let Ok(content) = std::fs::read_to_string(yaml_path) {
                let mut desc = String::new();
                for line in content.lines() {
                    if line.trim().starts_with("description:") && final_context.is_empty() {
                        desc = line.splitn(2, ':').nth(1).unwrap_or("").trim().trim_matches('"').trim_matches('\'').to_string();
                        break;
                    }
                }
                
                if final_context.is_empty() && !desc.is_empty() {
                    final_context.push_str(&format!("Tool: {}\nDescription: {}\n", skill_name, desc));
                }
                
                let mut cel_grammar = String::new();
                for line in content.lines() {
                    if line.trim().starts_with("cel_grammar:") {
                        cel_grammar = line.splitn(2, ':').nth(1).unwrap_or("").trim().trim_matches('"').trim_matches('\'').to_string();
                        break;
                    }
                }
                
                let rule_str = match *tool_type {
                    "extension" => crate::neural_foundry::registry::injectors::ExtensionRuleInjector::compile_rules(&skill_name, &cel_grammar),
                    "plugin" => crate::neural_foundry::registry::injectors::PluginRuleInjector::compile_rules(&skill_name, &cel_grammar),
                    "mcp" => crate::neural_foundry::registry::injectors::McpRuleInjector::compile_rules(&skill_name),
                    _ => String::new(),
                };
                
                // Natively force-inject the Kernel tag instructions
                final_context.push_str(&format!("\n{}", rule_str));
                break;
            }
        }
    }

    if !final_context.is_empty() {
        return Some(final_context);
    }

    None
}

