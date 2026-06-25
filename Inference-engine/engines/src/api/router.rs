//! router.rs: The Core Dispatcher.
//! Routes prompts to the appropriate backend based on model architecture.

use std::path::PathBuf;
use crate::utils::healer::AutoHealer;
use cluaize_shared::{UnifiedBackend, BackendType, CluaizeContext, StructuralDNA, TemplateManager, ModelWeightsWrapper, CluaizeInference};
use crate::runtime::execution::hub::HardwareOrchestrator;
use neural_core::interfaces::router_contract::EmbeddingDriver;

/// Represents the architectural branch taken when generating a response.
/// This is observable state used for integration test verification.
#[derive(Debug, Clone, PartialEq)]
pub enum RouteDecision {
    /// No skill matched; plain LLM generation.
    NoSkill,
    /// Skill matched. Context was sufficient — inject immediately, compile KV in background.
    ZeroDelayTTFT { skill_id: String },
    /// Skill matched. Context was insufficient — Agentic Pause triggered, CPU prefill, then resume.
    AgenticPause { skill_id: String, success: bool },
    /// Skill matched. Warm KV cache already existed — loaded directly.
    WarmCacheHit { skill_id: String },
}

pub enum Backend {
    Empty(DummyBackend),
    Cluaize(ModelWeightsWrapper),
}

impl UnifiedBackend for Backend {
    fn generate(&mut self, prompt: &str, max_tokens: usize) -> Result<String, String> {
        match self {
            Self::Empty(b) => b.generate(prompt, max_tokens),
            Self::Cluaize(b) => b.generate(prompt, max_tokens),
        }
    }
    fn prefill(&mut self, prompt: &str) -> anyhow::Result<()> {
        match self {
            Self::Empty(b) => b.prefill(prompt),
            Self::Cluaize(b) => b.prefill(prompt),
        }
    }

    fn evaluate_tps(&self) -> f64 {
        match self {
            Self::Empty(b) => b.evaluate_tps(),
            Self::Cluaize(b) => b.evaluate_tps(),
        }
    }
    
    fn embed(&mut self, input: &str) -> anyhow::Result<Vec<f32>> {
        match self {
            Self::Empty(b) => b.embed(input),
            Self::Cluaize(b) => b.embed(input),
        }
    }
}

impl cluaize_shared::CluaizeInference for Backend {
    fn forward_raw(&mut self, inputs: &[u32], pos: usize) -> anyhow::Result<Vec<f32>> {
        match self {
            Self::Cluaize(b) => b.forward_raw(inputs, pos),
            Self::Empty(_) => Err(anyhow::anyhow!("Empty backend")),
        }
    }

    fn generate_stream(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Cluaize(b) => b.generate_stream(prompt, max_tokens, callback),
            Self::Empty(_) => Err(anyhow::anyhow!("Empty backend")),
        }
    }

    fn dump_kv_cache(&mut self, path: &str) -> anyhow::Result<()> {
        match self {
            Self::Cluaize(b) => b.dump_kv_cache(path),
            Self::Empty(_) => Err(anyhow::anyhow!("Empty backend")),
        }
    }

    fn load_kv_cache(&mut self, path: &str) -> anyhow::Result<()> {
        match self {
            Self::Cluaize(b) => b.load_kv_cache(path),
            Self::Empty(_) => Err(anyhow::anyhow!("Empty backend")),
        }
    }

    fn inject_signals(&mut self, signals: Vec<cluaize_shared::hardware::memory::kv_cache::stitching::CluaizeSignal>) -> anyhow::Result<()> {
        match self {
            Self::Cluaize(b) => b.inject_signals(signals),
            Self::Empty(_) => Err(anyhow::anyhow!("Empty backend")),
        }
    }

    fn apply_booster(&mut self, control: &cluaize_shared::hardware::schema::booster::BoosterControl) -> anyhow::Result<()> {
        match self {
            Self::Cluaize(b) => b.apply_booster(control),
            Self::Empty(_) => Err(anyhow::anyhow!("Empty backend")),
        }
    }
}

pub struct CoreRouter {
    pub active_backend: Backend,
    pub active_dna: Option<cluaize_shared::StructuralDNA>,
    pub active_model_path: Option<PathBuf>,
    pub foundry: crate::neural_foundry::CoreFoundry,

    /// The routing decision taken on the last call to generate_stream.
    /// None if generate_stream has not been called yet.
    pub last_route_decision: Option<RouteDecision>,
    /// The REAL hardware-negotiated context size (set once at load_model time).
    /// active_dna.max_context_length may be overridden by tests; this is immutable.
    pub hardware_n_ctx: usize,
}

impl Default for CoreRouter {
    fn default() -> Self {
        Self::new()
    }
}

static COMPILATION_LOCKS: std::sync::LazyLock<std::sync::RwLock<std::collections::HashSet<PathBuf>>> = std::sync::LazyLock::new(|| std::sync::RwLock::new(std::collections::HashSet::new()));

struct CompilationGuard {
    path: PathBuf,
}

impl Drop for CompilationGuard {
    fn drop(&mut self) {
        if let Ok(mut locks) = COMPILATION_LOCKS.write() {
            locks.remove(&self.path);
            cluaize_shared::dev_info!("🔓 [Arbiter] Compilation lock released for: {:?}", self.path);
        }
    }
}

impl CoreRouter {
    pub fn new() -> Self {
        Self { 
            active_backend: Backend::Empty(DummyBackend),
            foundry: crate::neural_foundry::CoreFoundry::new(),
            active_dna: None,
            active_model_path: None,

            last_route_decision: None,
            hardware_n_ctx: 2048,
        }
    }

    pub async fn load_model(path: PathBuf, runtime: BackendType) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            let mut repo_id = path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default().to_string();
            let manifest_path = parent.join("model_manifest.json");
            if manifest_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                    if let Ok(manifest) = serde_json::from_str::<crate::models::registry::ModelManifest>(&content) {
                        if manifest.download_url.contains("huggingface.co/") {
                            repo_id = manifest.download_url
                                .split("huggingface.co/")
                                .nth(1)
                                .unwrap_or("")
                                .split("/resolve")
                                .next()
                                .unwrap_or(&repo_id)
                                .to_string();
                        }
                    }
                }
            }
            let _ = AutoHealer::heal_missing_tokenizer(&repo_id, parent).await;
        }

        // [Cluaize ALIGNMENT]: Bootstrapping context with local DNA and Templates
        let mut dna = StructuralDNA::default();
        if let Some(parent) = path.parent() {
            let dna_path = parent.join("structural_dna.json");
            if dna_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&dna_path) {
                    if let Ok(loaded_dna) = serde_json::from_str::<StructuralDNA>(&content) {
                        dna = loaded_dna;
                        cluaize_shared::dev_info!("🧬 [Router] Neural DNA synchronized from local manifest.");
                    }
                }
            }
            // 🧬 Deep Discovery: Learn and Repair from tokenizer_config.json, etc.
            dna.discover_from_path(parent)
                .map_err(|e| format!("Neural Discovery Failure: {}", e))?;
        }
        dna.preferred_runtime = Some(runtime);
        
        let context = CluaizeContext::boot(
            dna.clone(),
            TemplateManager::default(),
        );

        // 🚀 THE Cluaize HANDSHAKE: Dispatching to the Dynamic Linker
        cluaize_shared::dev_info!("🧬 [Router] Dispatching to HardwareOrchestrator for dynamic linkage...");
        let engine = HardwareOrchestrator::instantiate(&path.to_string_lossy(), "llama", context)
            .await
            .map_err(|e| format!("Cluaize Handshake Failure: {}", e))?;



        let mut foundry = crate::neural_foundry::CoreFoundry::new();
        // Load skills using EnvironmentManager
        let skills_dir = cluaize_shared::environment::EnvironmentManager::current().skills_dir();
        foundry.initialize(&skills_dir.to_string_lossy());


        // Capture the hardware-negotiated context BEFORE active_dna can be overridden by tests.
        let hardware_n_ctx = dna.max_context_length.unwrap_or(2048) as usize;

        Ok(Self { 
            active_backend: Backend::Cluaize(engine), 
            foundry,
            active_dna: Some(dna),
            active_model_path: Some(path),

            last_route_decision: None,
            hardware_n_ctx,
        })
    }

    pub fn ensure_skills_indexed(&self) {
        let schema = crate::neural_foundry::security::permission_schema::PermissionSchema::load();
        if schema.get_active_embedding_model().is_none() {
            return;
        }

        if let Ok(mut skill_router) = cluaize_shared::skills::router::GLOBAL_SKILL_ROUTER.write() {
            let _ = skill_router.boot_index();
            let mut new_vectors = Vec::new();
            let safe_filename = schema.get_active_embedding_model().unwrap_or_default().replace(":", "-");

            for (id, skill_manifest) in &skill_router.loaded_manifests {
                let skill_path = cluaize_shared::environment::EnvironmentManager::current().skills_dir().join(&skill_manifest.name);
                let cache_dir = skill_path.join(".cache");
                let emb_path = cache_dir.join(format!("{}.emb.bin", safe_filename));
                let norm_skill_path = cluaize_shared::skills::router::normalize_path(&skill_path);
                let has_vector = skill_router.skill_vectors.contains_key(&norm_skill_path);

                if !has_vector || !emb_path.exists() {
                    cluaize_shared::dev_info!("⏳ [Sovereign-Ops] Vector Mismatch. Generating semantic vector for skill: {}", skill_manifest.name);
                    let mut combined_vec = Vec::new();
                    
                    if skill_manifest.triggers.semantic.is_empty() {
                        if let Some(vec) = crate::memory::embedding_generator::EmbeddingGenerator::generate_full_vector(&skill_manifest.name) {
                            combined_vec.extend_from_slice(&vec);
                        }
                    } else {
                        for trigger in &skill_manifest.triggers.semantic {
                            if let Some(vec) = crate::memory::embedding_generator::EmbeddingGenerator::generate_full_vector(trigger) {
                                combined_vec.extend_from_slice(&vec);
                            }
                        }
                    }

                    if !combined_vec.is_empty() {
                        let _ = std::fs::create_dir_all(&cache_dir);
                        let data_bytes = unsafe { std::slice::from_raw_parts(combined_vec.as_ptr() as *const f32 as *const u8, combined_vec.len() * 4) };
                        if let Ok(_) = std::fs::write(&emb_path, data_bytes) {
                            new_vectors.push((norm_skill_path, combined_vec));
                        }
                    }
                }
            }
            
            for (p, v) in new_vectors {
                skill_router.skill_vectors.insert(p, v);
            }
        }
    }

    pub fn get_active_dna(&self) -> Option<&cluaize_shared::StructuralDNA> {
        self.active_dna.as_ref()
    }

    pub fn generate(&mut self, prompt: &str, max_tokens: usize) -> Result<String, String> {
        let formatted_prompt = prompt.to_string(); // Let native engines handle formatting
        self.active_backend.generate(&formatted_prompt, max_tokens)
    }

    pub fn generate_stream(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> Result<(), String> {

        let rt = tokio::runtime::Handle::current();

        // 🚀 Ensure skill embeddings exist
        self.ensure_skills_indexed();

        // 🚀 SLIDING WINDOW SEMANTIC SEARCH & ROUTING
        let mut matched_skill_ids = Vec::new();
        let prompt_lower = prompt.to_lowercase().trim().to_string();
        
        // 1. Text Match Fast-Path (Exact keyword match + Substring containment)
        if let Ok(router) = cluaize_shared::skills::router::GLOBAL_SKILL_ROUTER.read() {
            // Try exact full-prompt match first
            if let Some(path) = router.check_trigger(&prompt_lower) {
                if let Some(name) = path.file_name() {
                    matched_skill_ids.push(name.to_string_lossy().to_string());
                }
            }
            
            // If no exact match, try substring containment: check if prompt CONTAINS any registered trigger phrase
            if matched_skill_ids.is_empty() {
                for (keyword, path) in &router.keyword_index {
                    if prompt_lower.contains(keyword) {
                        if let Some(name) = path.file_name() {
                            matched_skill_ids.push(name.to_string_lossy().to_string());
                            break;
                        }
                    }
                }
            }
        }
        
        // 2. Sliding Window Semantic Match (ONNX vector similarity)
        if matched_skill_ids.is_empty() {
             // Since we use the global EmbeddingGenerator, we don't need a local onnx_engine
             if let Some(full_vec) = crate::memory::embedding_generator::EmbeddingGenerator::generate_full_vector(prompt) {
                 if let Ok(router) = cluaize_shared::skills::router::GLOBAL_SKILL_ROUTER.read() {
                     // Try full prompt vector first
                     if let Some(path) = router.check_semantic_trigger(&full_vec, 0.70) {
                         if let Some(name) = path.file_name() {
                             matched_skill_ids.push(name.to_string_lossy().to_string());
                         }
                     } else {
                         // Multi-size sliding window search: try window sizes 2, 3, 4
                         let words: Vec<&str> = prompt.split_whitespace().collect();
                         'window_search: for window_size in [2, 3, 4] {
                             if words.len() >= window_size {
                                 for i in 0..=words.len() - window_size {
                                     let chunk = words[i..i + window_size].join(" ");
                                     if let Some(chunk_vec) = crate::memory::embedding_generator::EmbeddingGenerator::generate_full_vector(&chunk) {
                                         if let Some(path) = router.check_semantic_trigger(&chunk_vec, 0.70) {
                                             if let Some(name) = path.file_name() {
                                                 matched_skill_ids.push(name.to_string_lossy().to_string());
                                             }
                                             break 'window_search;
                                         }
                                     }
                                 }
                             }
                         }
                     }
                 }
             }
        }

        // 🧪 Cluaize HANDSHAKE: Process Foundry Intent
        let mut intent_result = std::thread::scope(|s| {
            s.spawn(|| {
                rt.block_on(self.foundry.process_intent(prompt, Some(matched_skill_ids.clone())))
            }).join().unwrap()
        }).map_err(|e| format!("Skill Discovery Error: {}", e))?;

        // 🧠 CONTEXT BUDGET CALCULATION
        let n_ctx = self.active_dna.as_ref().and_then(|dna| dna.max_context_length).unwrap_or(2048) as usize;
        let system_tokens = 128; // System tool prompt rules
        let history_tokens = 0; // For now
        let reserve_for_generation = max_tokens.min(512); // Limit generation reserve to prevent context saturation
        let prompt_tokens_est = prompt.len() / 3;
        let available_ctx = n_ctx.saturating_sub(history_tokens + system_tokens + prompt_tokens_est + reserve_for_generation);
        
        // Reset route decision for this call
        self.last_route_decision = if matched_skill_ids.is_empty() { Some(RouteDecision::NoSkill) } else { None };

        // 🚀 TRUE AGENTIC PAUSE (FFI Hardware Spin-up)
        for (cache_path, skill_content) in intent_result.missing_caches.drain(..) {
            let skill_tokens_est = skill_content.len() / 3;
            
            // Look up skill metadata from registry to get exact size and head dimension
            let skill_path = cache_path.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf());
            let mut head_dim = 128;
            let mut token_count_override = skill_tokens_est;
            if let Some(ref path) = skill_path {
                if let Some(skill) = self.foundry.registry.skills.iter().find(|s| s.path == *path) {
                    if let Some(meta) = &skill.manifest.Core_metadata {
                        head_dim = meta.head_dim;
                        token_count_override = meta.token_count;
                    }
                }
            }

            // Check if this cache path is already compilation locked
            let is_compiling = {
                let locks = COMPILATION_LOCKS.read().unwrap();
                locks.contains(&cache_path)
            };

            if is_compiling {
                cluaize_shared::dev_info!("⏳ [Arbiter] Cache compilation for {} is already in progress. Skipping duplicate Agentic Pause.", cache_path.display());
                continue;
            }

            if skill_tokens_est > available_ctx {
                cluaize_shared::dev_info!("⏳ [Agentic Pause] Low Context Window detected ({} available). Spawning isolated hardware slot for {} tokens...", available_ctx, skill_tokens_est);
                
                // Extract skill id for telemetry before the closure moves cache_path
                let skill_id_for_decision = cache_path
                    .parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                
                if let Some(model_path) = &self.active_model_path {
                    let path_clone = model_path.clone();
                    let cache_path_clone = cache_path.clone();
                    let skill_content_clone = skill_content.clone();
                    let expanded_ctx = (skill_tokens_est + 256) as usize; // Exact tailored slot
                    
                    // Acquire compilation lock
                    {
                        let mut locks = COMPILATION_LOCKS.write().unwrap();
                        locks.insert(cache_path.clone());
                    }
                    
                    let background_success = std::thread::scope(|s| {
                        s.spawn(|| {
                            rt.block_on(async move {
                                let _guard = CompilationGuard { path: cache_path_clone.clone() };
                                use cluaize_shared::{CluaizeContext, StructuralDNA, UnifiedBackend, CluaizeInference};
                                let mut temp_dna = StructuralDNA::default();
                                temp_dna.max_context_length = Some(expanded_ctx);
                                let ctx = CluaizeContext::boot(temp_dna, cluaize_shared::TemplateManager::default());
                                
                                cluaize_shared::dev_info!("🔩 [Arbiter] Requesting {} ctx slot in background (CPU fallback mode)...", expanded_ctx);
                                let mut booster = cluaize_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();
                                booster.n_gpu_layers = 0; // Force CPU-only to avoid CUDA device context collisions
                                
                                if let Ok(mut bg_engine) = crate::runtime::execution::hub::HardwareOrchestrator::instantiate_with_booster(
                                    &path_clone.to_string_lossy(),
                                    "llama", 
                                    ctx,
                                    Some(booster)
                                ).await {
                                    cluaize_shared::dev_info!("⚙️ [Arbiter] Background slot acquired. Prefilling {} tokens...", skill_content_clone.len() / 3);
                                    if bg_engine.prefill(&skill_content_clone).is_ok() {
                                        let _ = bg_engine.dump_kv_cache(&cache_path_clone.to_string_lossy());
                                        return true;
                                    }
                                }
                                false
                            })
                        }).join().unwrap()
                    });
                    
                    // Record the Agentic Pause decision with success/failure outcome
                    self.last_route_decision = Some(RouteDecision::AgenticPause {
                        skill_id: skill_id_for_decision,
                        success: background_success,
                    });
                    
                    if background_success {
                        cluaize_shared::dev_info!("✅ [Agentic Pause] Dual-Cache `.kvcache.bin` safely generated to SSD.");
                        // Only attempt KV load if the cache was saved at a size the main engine can handle.
                        // The background engine used expanded_ctx tokens, but main engine has hardware_n_ctx.
                        if expanded_ctx <= self.hardware_n_ctx {
                            cluaize_shared::dev_info!("⚙️ [Arbiter] Loading KV cache natively from SSD: {}", cache_path.display());
                            if let Err(e) = self.active_backend.load_kv_cache(&cache_path.to_string_lossy()) {
                                cluaize_shared::dev_info!("❌ [Arbiter] Native KV load failed: {}. Force-resetting memory.", e);
                                // 🛡️ Force full memory reset to prevent hybrid SSM/KV state corruption
                                let _ = self.active_backend.prefill("");
                            }
                        } else {
                            cluaize_shared::dev_info!("⚠️ [Arbiter] KV cache was saved at {} ctx but engine has {} ctx. Skipping load (would corrupt hybrid memory).",
                                expanded_ctx, self.hardware_n_ctx);
                        }
                    } else {
                        cluaize_shared::dev_info!("❌ [Agentic Pause] Hardware failed to acquire background slot. Proceeding safely without skill.");
                    }
                }
            } else {
                // Case B: Fits entirely in current available_ctx! Zero-Delay TTFT!
                let skill_id_for_decision = cache_path
                    .parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.last_route_decision = Some(RouteDecision::ZeroDelayTTFT { skill_id: skill_id_for_decision });

                // ⚠️ Hardware Guard: Cap injected text to the REAL engine context window.
                // active_dna.max_context_length may have been set artificially high (e.g. in tests),
                // but the engine's KV cache is bounded by hardware_n_ctx (set at load time).
                // Reserve 512 tokens for prompt + generation headroom.
                let injection_char_cap = self.hardware_n_ctx.saturating_sub(512) * 3;
                let safe_skill_content = if skill_content.len() > injection_char_cap && injection_char_cap > 0 {
                    cluaize_shared::dev_info!("⚠️ [ZeroDelayTTFT] Skill ({} chars) exceeds hardware context cap ({} chars). Truncating for safe injection.",
                        skill_content.len(), injection_char_cap);
                    skill_content[..injection_char_cap].to_string()
                } else {
                    skill_content.clone()
                };
                intent_result.responses.push(safe_skill_content);
                if let Some(model_path) = &self.active_model_path {
                    let path_clone = model_path.clone();
                    let cache_path_clone = cache_path.clone();
                    let skill_content_clone = skill_content.clone();
                    let expanded_ctx = (skill_tokens_est + 256) as usize;
                    
                    // Acquire compilation lock
                    {
                        let mut locks = COMPILATION_LOCKS.write().unwrap();
                        locks.insert(cache_path.clone());
                    }
                    
                    rt.spawn(async move {
                        let _guard = CompilationGuard { path: cache_path_clone.clone() };
                        use cluaize_shared::{CluaizeContext, StructuralDNA, UnifiedBackend, CluaizeInference};
                        let mut temp_dna = StructuralDNA::default();
                        temp_dna.max_context_length = Some(expanded_ctx);
                        let ctx = CluaizeContext::boot(temp_dna, cluaize_shared::TemplateManager::default());
                        
                        cluaize_shared::dev_info!("🔩 [Arbiter] Asynchronously requesting {} ctx slot in background...", expanded_ctx);
                        let mut booster = cluaize_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();
                        booster.n_gpu_layers = 0; // Force CPU-only to avoid CUDA device context collisions
                        
                        if let Ok(mut bg_engine) = crate::runtime::execution::hub::HardwareOrchestrator::instantiate_with_booster(
                            &path_clone.to_string_lossy(),
                            "llama", 
                            ctx,
                            Some(booster)
                        ).await {
                            cluaize_shared::dev_info!("⚙️ [Arbiter] Async background slot acquired. Prefilling {} tokens...", skill_content_clone.len() / 3);
                            if bg_engine.prefill(&skill_content_clone).is_ok() {
                                let _ = bg_engine.dump_kv_cache(&cache_path_clone.to_string_lossy());
                                cluaize_shared::dev_info!("✅ [Arbiter] Async background KV cache compiled and dumped successfully.");
                            }
                        }
                    });
                }
            }
        }

        // If the cache already exists, load it natively (WarmCacheHit path)
        let schema = crate::neural_foundry::security::permission_schema::PermissionSchema::load();
        if let Some(gen_model) = schema.get_active_chat_model() {
            let gen_model_safe = gen_model.replace(":", "-");
            for skill_id in &matched_skill_ids {
                if let Some(skill) = self.foundry.registry.skills.iter().find(|s| &s.manifest.id == skill_id) {
                    let cache_dir = skill.path.join(".cache");
                    let kv_cache_path = cache_dir.join(format!("{}.kvcache.bin", gen_model_safe));
                    if kv_cache_path.exists() {
                        // Check if the skill's token count exceeds what the main engine can actually load
                        let skill_tokens_est = skill.manifest.Core_metadata.as_ref()
                            .map(|m| m.token_count)
                            .unwrap_or(0);
                        if skill_tokens_est > 0 && skill_tokens_est + 256 > self.hardware_n_ctx {
                            cluaize_shared::dev_info!("⚠️ [Arbiter] Warm cache for '{}' was saved at ~{} tokens but engine has {} ctx. Skipping load.",
                                skill_id, skill_tokens_est + 256, self.hardware_n_ctx);
                            continue;
                        }
                        cluaize_shared::dev_info!("⚙️ [Arbiter] Warm cache found. Loading KV cache natively from SSD: {}", kv_cache_path.display());
                        // Only override if Agentic Pause didn't already set the decision
                        if self.last_route_decision.is_none() {
                            self.last_route_decision = Some(RouteDecision::WarmCacheHit { skill_id: skill_id.clone() });
                        }
                        if let Err(e) = self.active_backend.load_kv_cache(&kv_cache_path.to_string_lossy()) {
                            cluaize_shared::dev_info!("❌ [Arbiter] Native warm KV load failed: {}. Force-resetting memory.", e);
                            let _ = self.active_backend.prefill("");
                        }
                    }
                }
            }
        }

        match &mut self.active_backend {
            Backend::Cluaize(b) => {

                // If Core signals (skill souls) were identified, inject them into the kernel
                if !intent_result.signals.is_empty() {
                    cluaize_shared::dev_info!("💉 [Router] Injecting {} M-RoPE KV Cache signals into active hardware...", intent_result.signals.len());
                    b.inject_signals(intent_result.signals).map_err(|e| format!("Signal Injection Failure: {}", e))?;
                }

                let mut formatted_prompt = prompt.to_string();
                
                if !intent_result.responses.is_empty() {
                    let tool_context = intent_result.responses.join("\n\n");
                    formatted_prompt = format!(
                        "<system>\nYou have access to the following skills/tools. Read their documentation carefully and use them if needed to fulfill the user's request. Output valid JSON to use a tool if instructed by the skill documentation:\n\n{}\n</system>\n\n{}", 
                        tool_context, 
                        formatted_prompt
                    );
                    cluaize_shared::dev_info!("🧠 [Router] Injected skill descriptions (Zero-Delay TTFT).");
                }

                b.generate_stream(&formatted_prompt, max_tokens, callback)
                    .map_err(|e| e.to_string())
            },
            Backend::Empty(_) => Err("Core weights not loaded. Please select a model with @ or wait for the Auto-Pilot handshake to complete.".to_string()),
        }
    }
    
    pub fn embed(&mut self, input: &str) -> Result<Vec<f32>, String> {
        self.active_backend.embed(input).map_err(|e| e.to_string())
    }
}

pub struct DummyBackend;
impl cluaize_shared::UnifiedBackend for DummyBackend {
    fn generate(&mut self, _prompt: &str, _max_tokens: usize) -> Result<String, String> {
        Err("Core weights not loaded.".to_string())
    }
    fn prefill(&mut self, _prompt: &str) -> anyhow::Result<()> { Ok(()) }
    fn evaluate_tps(&self) -> f64 { 0.0 }
}

impl cluaize_shared::CluaizeInference for DummyBackend {
    fn forward_raw(&mut self, _inputs: &[u32], _pos: usize) -> anyhow::Result<Vec<f32>> {
        Err(anyhow::anyhow!("Dummy backend"))
    }
    fn generate_stream(
        &mut self,
        _prompt: &str,
        _max_tokens: usize,
        _callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("Dummy backend"))
    }

    fn load_kv_cache(&mut self, _path: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("Dummy backend"))
    }
}

fn extract_frontmatter(skill_dir: &std::path::Path) -> Option<String> {
    let skill_md_path = skill_dir.join("SKILL.md");
    if skill_md_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&skill_md_path) {
            let lines: Vec<&str> = content.lines().collect();
            let mut start_idx = None;
            let mut end_idx = None;
            for (i, line) in lines.iter().enumerate() {
                if line.trim() == "---" {
                    if start_idx.is_none() {
                        start_idx = Some(i);
                    } else {
                        end_idx = Some(i);
                        break;
                    }
                }
            }
            if let (Some(start), Some(end)) = (start_idx, end_idx) {
                if end > start + 1 {
                    let frontmatter_lines = &lines[start + 1..end];
                    return Some(frontmatter_lines.join("\n"));
                }
            }
        }
    }
    None
}
