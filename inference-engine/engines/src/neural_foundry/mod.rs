// cluaiz-engine: Core Foundry - The cluaiz Engine Core
// Clean integration of Runtime, Security, and Ingestion.

pub mod runtime;
pub mod security;
pub mod ingestion;

use runtime::wasm_host::WasmHost;
use runtime::mcp_gateway::McpGateway;
use security::guard::{PermissionGuard, PermissionLevel};
use tracing::{info, warn};
use cluaiz_shared::hardware::memory::kv_cache::stitching::cluaizSignal;
use std::sync::Mutex;
use std::path::PathBuf;

pub struct IntentResult {
    pub responses: Vec<String>,
    pub signals: Vec<cluaizSignal>,
    pub missing_caches: Vec<(PathBuf, String)>, // (kv_cache_path, skill_content)
}

pub struct CoreFoundry {
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
            wasm_runtime: WasmHost::new(),
            mcp_gateway: McpGateway::new(),
            guard: PermissionGuard::new(),
            active_skill_ids: Mutex::new(Vec::new()),
        }
    }

    pub fn initialize(&mut self, _skills_dir: &str) {
        let env = cluaiz_shared::environment::EnvironmentManager::current();
        cluaiz_shared::dev_info!("[cluaiz] Initializing Core Foundry with tools at: {}", env.tools_dir().display());
    }

    /// The cluaiz Flow: Prompt -> Multi-Route -> Execute
    pub async fn process_intent(&self, prompt: &str, pre_matched_skills: Option<Vec<String>>) -> anyhow::Result<IntentResult> {
        let skill_ids = pre_matched_skills.unwrap_or_else(|| crate::tools::ToolsEngine::match_skills(prompt));
        let mut result = IntentResult { responses: Vec::new(), signals: Vec::new(), missing_caches: Vec::new() };

        if skill_ids.is_empty() {
            return Ok(result);
        }

        info!("🧪 [CoreFoundry] Multi-Skill Fusion Active: {} skills detected.", skill_ids.len());

        for (i, skill_id) in skill_ids.iter().enumerate() {
            let tool = match crate::tools::ToolsEngine::get_tool(skill_id).unwrap_or(None) {
                Some(t) => t,
                None => continue,
            };

            let skill_path = PathBuf::from(&tool.local_dir);
            let skill_description = tool.description.clone();
            
            // 2. Dynamic Memory Management (RAM/VRAM Bounding)
            {
                let pulse = cluaiz_shared::hardware::telemetry::get_pulse();
                let pulse_lock = pulse.pulse.read().unwrap();
                let used_mb = pulse_lock.ram.used_gb * 1024.0;
                let util = pulse_lock.ram.utilization_pct as f64;
                let available_ram_mb = if util > 0.1 {
                    (used_mb / (util / 100.0)) - used_mb
                } else {
                    8192.0
                };
                
                let skill_est_size_mb = 50.0; 

                let mut active_ids = self.active_skill_ids.lock().unwrap();
                
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
            let skill_id_clone = skill_id.clone();
            let skill_path_clone = skill_path.clone();

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
                        let content = crate::tools::ToolsEngine::get_skill_instructions(&skill_id_clone)
                            .unwrap_or_else(|| skill_description.clone());
                        return SkillLoadResult::TextPayload { content };
                    }
                    
                    let cache_exists = kv_cache_path.exists();

                    if cache_exists {
                        use cluaiz_shared::hardware::memory::buffer::SafeTensorsMappedBuffer;
                        
                        if let Ok(mapped_buffer) = SafeTensorsMappedBuffer::from_file(&kv_cache_path) {
                            return SkillLoadResult::Signal {
                                raw_data: mapped_buffer,
                                token_count: 0,
                                head_dim: 128,
                            };
                        }
                    } else {
                        tracing::warn!("⚠️ [CoreFoundry] {} missing for skill {}. Flagging for Sovereign Compiler.", kv_cache_path.display(), skill_id_clone);
                        
                        let content = extract_skill_body(&skill_path_clone)
                            .unwrap_or_else(|| skill_description.clone());
                        
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
                        raw_data: std::sync::Arc::new(raw_data),
                        token_count,
                        head_dim,
                    });
                }
                SkillLoadResult::MissingCache { kv_cache_path, content } => {
                    result.missing_caches.push((kv_cache_path, content.clone()));
                    result.responses.push(content);
                }
                SkillLoadResult::TextPayload { content } => {
                    result.responses.push(content);
                }
                SkillLoadResult::NoModel => {
                    warn!("⚠️ [CoreFoundry] No text model assigned in permission.json. Skipping Zero-Copy injection for skill {}.", skill_id);
                }
                SkillLoadResult::None => {}
            }
            
            // WASM logic execution is handled during streaming/generation interceptor.
            self.guard.validate_action(&skill_id, PermissionLevel::ReadOnly)?;
        }
        
        Ok(result)
    }
}

pub fn extract_skill_body(skill_dir: &std::path::Path) -> Option<String> {
    let skill_name = skill_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if let Some(instructions) = crate::tools::ToolsEngine::get_skill_instructions(&skill_name) {
        return Some(instructions);
    }

    let skill_md_path = skill_dir.join("SKILL.md");
    if skill_md_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&skill_md_path) {
            return Some(content);
        }
    }

    None
}
