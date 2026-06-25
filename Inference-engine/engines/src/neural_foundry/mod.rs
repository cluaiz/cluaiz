// cluaize-engine: Core Foundry - The Cluaize Engine Core
// Final integration of Registry, Intelligence, Runtime, and Security.

pub mod registry;
pub mod intelligence;
pub mod runtime;
pub mod security;
pub mod ingestion;

use registry::SkillRegistry;
use intelligence::skill_router::SkillRouter;
use runtime::wasm_host::WasmHost;
use runtime::mcp_gateway::McpGateway;
use security::guard::{PermissionGuard, PermissionLevel};
use tracing::{info, warn};
use cluaize_shared::hardware::memory::kv_cache::stitching::CluaizeSignal;
use neural_core::interfaces::memory_contract::MappedBuffer;
use std::sync::{Mutex, Arc};
use std::path::PathBuf;

// Removed fixed MAX_ACTIVE_SKILLS limit. Bounding is now purely dynamic based on hardware capacity.
// Constant threshold for the ONNX semantic similarity match.

pub struct IntentResult {
    pub responses: Vec<String>,
    pub signals: Vec<CluaizeSignal>,
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

    /// Initializes the foundry by scanning the skills directory.
    pub fn initialize(&mut self, skills_dir: &str) {
        cluaize_shared::dev_info!("[Cluaize] Initializing Core Foundry from: {}", skills_dir);
        self.registry.load_from_directory(skills_dir);
    }

    /// The Cluaize Flow: Prompt -> Multi-Route -> Execute
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
                let pulse = cluaize_shared::hardware::telemetry::get_pulse();
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
                        cluaize_shared::dev_info!("[Cluaize] [VRAM] Bounding limit hit. Evicting LRU skill: {}", evicted_id);
                    } else {
                        break;
                    }
                }

                active_ids.retain(|id| id != skill_id);
                active_ids.push(skill_id.to_string());
            }

            // 3. Map Cluaize Signal (Zero-Copy Dual-Cache)
            // Offload the blocking disk I/O to a background thread to prevent blocking the async runtime
            let skill_id_clone = skill_id.clone();
            let skill_path_clone = skill.path.clone();
            let skill_manifest_clone = skill.manifest.clone();

            enum SkillLoadResult {
                Signal {
                    raw_data: MappedBuffer,
                    token_count: usize,
                    head_dim: usize,
                },
                MissingCache {
                    kv_cache_path: PathBuf,
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
                    let kv_cache_path = cache_dir.join(format!("{}.kvcache.bin", gen_model_safe));
                    
                    let mut cache_exists = kv_cache_path.exists();
                    let mut layers = None;
                    let mut kv_heads = None;

                    if cache_exists {
                        let roster = crate::models::registry::CoreRoster::load_roster();
                        if let Some(manifest) = roster.iter().find(|m| m.id == gen_model) {
                            if let Some(local_path) = &manifest.local_path {
                                let dna_path = std::path::Path::new(local_path).join("structural_dna.json");
                                if let Ok(dna_content) = std::fs::read_to_string(&dna_path) {
                                    if let Ok(dna) = serde_json::from_str::<cluaize_shared::StructuralDNA>(&dna_content) {
                                        layers = dna.layer_count;
                                        kv_heads = dna.attention_head_count_kv.or(dna.attention_head_count);
                                    }
                                }
                            }
                        }

                        let mut is_valid = true;
                        if let Ok(metadata) = std::fs::metadata(&kv_cache_path) {
                            let actual_size = metadata.len() as usize;
                            if actual_size == 0 {
                                is_valid = false;
                            } else if let (Some(l), Some(h)) = (layers, kv_heads) {
                                let head_dim = skill_manifest_clone.Core_metadata.as_ref().map_or(128, |m| m.head_dim);
                                let token_count = skill_manifest_clone.Core_metadata.as_ref().map_or(0, |m| m.token_count);
                                let expected = token_count * l * h * head_dim * 2 * 2;
                                if actual_size != expected {
                                    tracing::warn!("⚠️ [CoreFoundry] Cache size mismatch for {}: expected {} bytes, got {}. Evicting.", skill_id_clone, expected, actual_size);
                                    is_valid = false;
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
                        if let Ok(mapped_buffer) = MappedBuffer::from_file(&kv_cache_path) {
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
                    result.signals.push(CluaizeSignal {
                        raw_data: Arc::new(raw_data),
                        token_count,
                        head_dim,
                    });
                }
                SkillLoadResult::MissingCache { kv_cache_path, content } => {
                    result.missing_caches.push((kv_cache_path, content));
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

fn extract_skill_body(skill_dir: &std::path::Path) -> Option<String> {
    // ðŸ§  1. ZERO-LATENCY FFI BRAIN INJECTION
    // If the brain is enabled, it completely bypasses disk reads.
    if let Some(skill_name) = skill_dir.file_name().map(|s| s.to_string_lossy().to_string()) {
        if let Some(raw_bytes) = crate::memory::tensor_transducer::TensorTransducer::inject_context(&skill_name) {
            if let Ok(content) = String::from_utf8(raw_bytes) {
                return Some(content);
            }
        }
    }

    // ðŸ¢ 2. LEGACY DISK READ FALLBACK
    let skill_md_path = skill_dir.join("SKILL.md");
    if skill_md_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&skill_md_path) {
            let normalized = content.replace("\r\n", "\n");
            let lines: Vec<&str> = normalized.lines().collect();
            
            // Check if the file starts with frontmatter (first non-empty line is "---")
            let mut first_line_idx = None;
            for (i, line) in lines.iter().enumerate() {
                if !line.trim().is_empty() {
                    if line.trim() == "---" {
                        first_line_idx = Some(i);
                    }
                    break;
                }
            }

            if let Some(start_idx) = first_line_idx {
                // Find the closing "---" of the frontmatter
                let mut closing_idx = None;
                for i in (start_idx + 1)..lines.len() {
                    if lines[i].trim() == "---" {
                        closing_idx = Some(i);
                        break;
                    }
                }

                if let Some(end_idx) = closing_idx {
                    // Body starts after the closing "---"
                    let body_lines = &lines[end_idx + 1..];
                    let body = body_lines.join("\n").trim().to_string();
                    if !body.is_empty() {
                        return Some(body);
                    }
                }
            }

            let trimmed = content.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

