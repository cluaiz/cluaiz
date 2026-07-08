//! router.rs: The Core Dispatcher.
//! Routes prompts to the appropriate backend based on model architecture.

use std::path::PathBuf;
use crate::utils::healer::AutoHealer;
use cluaiz_shared::{UnifiedBackend, BackendType, cluaizContext, StructuralDNA, TemplateManager, ModelWeightsWrapper, cluaizInference};
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
    cluaiz(ModelWeightsWrapper),
}

impl UnifiedBackend for Backend {
    fn generate(&mut self, prompt: &str, max_tokens: usize) -> Result<String, String> {
        match self {
            Self::Empty(b) => b.generate(prompt, max_tokens),
            Self::cluaiz(b) => b.generate(prompt, max_tokens),
        }
    }
    fn prefill(&mut self, prompt: &str) -> anyhow::Result<()> {
        match self {
            Self::Empty(b) => b.prefill(prompt),
            Self::cluaiz(b) => b.prefill(prompt),
        }
    }

    fn evaluate_tps(&self) -> f64 {
        match self {
            Self::Empty(b) => b.evaluate_tps(),
            Self::cluaiz(b) => b.evaluate_tps(),
        }
    }
    
    fn embed(&mut self, input: &str) -> anyhow::Result<Vec<f32>> {
        match self {
            Self::Empty(b) => b.embed(input),
            Self::cluaiz(b) => b.embed(input),
        }
    }
}

impl cluaiz_shared::cluaizInference for Backend {
    fn forward_raw(&mut self, inputs: &[u32], pos: usize) -> anyhow::Result<Vec<f32>> {
        match self {
            Self::cluaiz(b) => b.forward_raw(inputs, pos),
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
            Self::cluaiz(b) => b.generate_stream(prompt, max_tokens, callback),
            Self::Empty(_) => Err(anyhow::anyhow!("Empty backend")),
        }
    }

    fn dump_kv_cache(&mut self, path: &str) -> anyhow::Result<()> {
        match self {
            Self::cluaiz(b) => b.dump_kv_cache(path),
            Self::Empty(_) => Err(anyhow::anyhow!("Empty backend")),
        }
    }

    fn load_kv_cache(&mut self, path: &str) -> anyhow::Result<()> {
        match self {
            Self::cluaiz(b) => b.load_kv_cache(path),
            Self::Empty(_) => Err(anyhow::anyhow!("Empty backend")),
        }
    }

    fn inject_signals(&mut self, signals: Vec<cluaiz_shared::hardware::memory::kv_cache::stitching::cluaizSignal>) -> anyhow::Result<()> {
        match self {
            Self::cluaiz(b) => b.inject_signals(signals),
            Self::Empty(_) => Err(anyhow::anyhow!("Empty backend")),
        }
    }

    fn apply_booster(&mut self, control: &cluaiz_shared::hardware::schema::booster::BoosterControl) -> anyhow::Result<()> {
        match self {
            Self::cluaiz(b) => b.apply_booster(control),
            Self::Empty(_) => Err(anyhow::anyhow!("Empty backend")),
        }
    }
}

pub struct CoreRouter {
    pub active_backend: Backend,
    pub active_backend_name: String,
    pub active_dna: Option<cluaiz_shared::StructuralDNA>,
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
            cluaiz_shared::dev_info!("🔓 [Arbiter] Compilation lock released for: {:?}", self.path);
        }
    }
}

impl CoreRouter {
    pub fn new() -> Self {
        Self { 
            active_backend: Backend::Empty(DummyBackend),
            active_backend_name: "llama".to_string(),
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

        // [cluaiz ALIGNMENT]: Bootstrapping context with local DNA and Templates
        let mut dna = StructuralDNA::default();
        if let Some(parent) = path.parent() {
            let dna_path = parent.join("structural_dna.json");
            if dna_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&dna_path) {
                    if let Ok(loaded_dna) = serde_json::from_str::<StructuralDNA>(&content) {
                        dna = loaded_dna;
                        cluaiz_shared::dev_info!("🧬 [Router] Neural DNA synchronized from local manifest.");
                    }
                }
            }
            // 🧬 Deep Discovery: Learn and Repair from tokenizer_config.json, etc.
            dna.discover_from_path(parent)
                .map_err(|e| format!("Neural Discovery Failure: {}", e))?;
        }
        dna.preferred_runtime = Some(runtime.clone());
        
        let context = cluaizContext::boot(
            dna.clone(),
            TemplateManager::default(),
        );

        let engine_str = match runtime {
            BackendType::RuntimeA => "onnx",
            BackendType::RuntimeB => "llama",
            BackendType::RuntimeC => "bitnet",
            BackendType::RuntimeD => "triton",
            BackendType::SwitchNode => "moe",
        };

        // 🚀 THE cluaiz HANDSHAKE: Dispatching to the Dynamic Linker
        cluaiz_shared::dev_info!("🧬 [Router] Dispatching to HardwareOrchestrator for dynamic linkage ({})...", engine_str);
        let engine = HardwareOrchestrator::instantiate(&path.to_string_lossy(), engine_str, context)
            .await
            .map_err(|e| format!("cluaiz Handshake Failure: {}", e))?;



        let mut foundry = crate::neural_foundry::CoreFoundry::new();
        // Load skills using EnvironmentManager
        let skills_dir = cluaiz_shared::environment::EnvironmentManager::current().skills_dir();
        foundry.initialize(&skills_dir.to_string_lossy());


        // Capture the hardware-negotiated context BEFORE active_dna can be overridden by tests.
        let hardware_n_ctx = dna.max_context_length.unwrap_or(2048) as usize;

        Ok(Self { 
            active_backend: Backend::cluaiz(engine),
            active_backend_name: engine_str.to_string(),
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

        if let Ok(mut skill_router) = cluaiz_shared::skills::router::GLOBAL_SKILL_ROUTER.write() {
            let _ = skill_router.boot_index();
            let mut new_vectors = Vec::new();
            let safe_filename = schema.get_active_embedding_model().unwrap_or_default().replace(":", "-").replace("/", "-").replace("\\", "-");

            for (id, skill_manifest) in &skill_router.loaded_manifests {
                let mut skill_path = cluaiz_shared::environment::EnvironmentManager::current().skills_dir().join(&skill_manifest.name);
                let env = cluaiz_shared::environment::EnvironmentManager::current();
                
                if let Some(skill) = self.foundry.registry.skills.iter().find(|s| s.manifest.id == *id) {
                    skill_path = skill.path.clone();
                } else {
                    for base_dir in [env.skills_dir(), env.extensions_dir(), env.plugins_dir(), env.mcp_dir()] {
                        let possible_path = base_dir.join(&skill_manifest.name);
                        if possible_path.exists() {
                            skill_path = possible_path;
                            break;
                        }
                    }
                }
                let cache_dir = skill_path.join(".cache");
                let emb_path = cache_dir.join(format!("{}.emb.safetensors", safe_filename));
                let norm_skill_path = cluaiz_shared::skills::router::normalize_path(&skill_path);
                let has_vector = skill_router.skill_vectors.get(&norm_skill_path).map_or(false, |v| !v.is_empty());
                let cache_file_valid = emb_path.exists() && std::fs::metadata(&emb_path).map(|m| m.len()).unwrap_or(0) > 0;

                if !has_vector || !cache_file_valid {
                    cluaiz_shared::dev_info!("⏳ [Sovereign-Ops] Vector Mismatch. Generating semantic vector for skill: {}", skill_manifest.name);
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

                    let _ = std::fs::create_dir_all(&cache_dir);
                    if !combined_vec.is_empty() {
                        let data_bytes = unsafe { std::slice::from_raw_parts(combined_vec.as_ptr() as *const f32 as *const u8, combined_vec.len() * 4) };
                        if let Ok(view) = safetensors::tensor::TensorView::new(
                            safetensors::tensor::Dtype::F32, 
                            vec![combined_vec.len()], 
                            data_bytes
                        ) {
                            let mut metadata = std::collections::HashMap::new();
                            metadata.insert("model".to_string(), safe_filename.clone());
                            
                            match safetensors::serialize_to_file(
                                vec![("embedding", view)], 
                                Some(metadata), 
                                &emb_path
                            ) {
                                Ok(_) => { new_vectors.push((norm_skill_path, combined_vec)); },
                                Err(e) => { cluaiz_shared::dev_info!("❌ Failed to write vector cache to {}: {}", emb_path.display(), e); },
                            }
                        }
                    } else {
                        cluaiz_shared::dev_info!("⚠️ Failed to generate vector (embedding engine returned empty). Skipping cache write.");
                    }
                }
            }
            
            for (p, v) in new_vectors {
                skill_router.skill_vectors.insert(p, v);
            }
        }
    }

    pub fn get_active_dna(&self) -> Option<&cluaiz_shared::StructuralDNA> {
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
        if let Ok(router) = cluaiz_shared::skills::router::GLOBAL_SKILL_ROUTER.read() {
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
        
        // 2. Sliding Window Semantic Match & Fallback triggers
        if matched_skill_ids.is_empty() {
             let schema = crate::neural_foundry::security::permission_schema::PermissionSchema::load();
             let active_model_opt = schema.get_active_embedding_model();
             
             // Dynamic Lazy-Loading Check: Check if prompt contains any registered trigger keywords first
             let mut has_trigger_keyword = false;
             let padded_prompt = format!(" {} ", prompt_lower);
             if let Ok(router) = cluaiz_shared::skills::router::GLOBAL_SKILL_ROUTER.read() {
                 for keyword in router.keyword_index.keys() {
                     let norm_keyword = format!(" {} ", keyword.to_lowercase().trim());
                     if padded_prompt.contains(&norm_keyword) {
                         has_trigger_keyword = true;
                         break;
                     }
                 }
             }

             if has_trigger_keyword && active_model_opt.is_some() {
                 if let Some(full_vec) = crate::memory::embedding_generator::EmbeddingGenerator::generate_full_vector(prompt) {
                     if let Ok(router) = cluaiz_shared::skills::router::GLOBAL_SKILL_ROUTER.read() {
                         if let Some(path) = router.check_semantic_trigger(&full_vec, 0.70) {
                             if let Some(name) = path.file_name() {
                                 matched_skill_ids.push(name.to_string_lossy().to_string());
                             }
                         }
                     }
                 }
             }

             // Fallback triggers containment match: check substring containment in prompt dynamically
             if matched_skill_ids.is_empty() {
                 if let Ok(router) = cluaiz_shared::skills::router::GLOBAL_SKILL_ROUTER.read() {
                     let prompt_lower = prompt.to_lowercase();
                     let padded_prompt = format!(" {} ", prompt_lower);
                     for (keyword, path) in &router.keyword_index {
                         let norm_keyword = format!(" {} ", keyword.to_lowercase().trim());
                         if padded_prompt.contains(&norm_keyword) {
                             if let Some(name) = path.file_name() {
                                 let id = name.to_string_lossy().to_string();
                                 if !matched_skill_ids.contains(&id) {
                                     matched_skill_ids.push(id);
                                 }
                             }
                         }
                     }
                 }
             }
        }

        // 🧪 cluaiz HANDSHAKE: Process Foundry Intent
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
                cluaiz_shared::dev_info!("⏳ [Arbiter] Cache compilation for {} is already in progress. Skipping duplicate Agentic Pause.", cache_path.display());
                continue;
            }

            if skill_tokens_est > available_ctx {
                cluaiz_shared::dev_info!("⏳ [Agentic Pause] Low Context Window detected ({} available). Spawning isolated hardware slot for {} tokens...", available_ctx, skill_tokens_est);
                
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
                    
                    // The path returned by Intent is .safetensors, we will pass it as is.
                    // Acquire compilation lock
                    {
                        let mut locks = COMPILATION_LOCKS.write().unwrap();
                        locks.insert(cache_path.clone());
                    }
                    
                    let backend_name_clone = self.active_backend_name.clone();
                    
                    let background_success = agentic_pause_compile_cache(
                        &rt,
                        cache_path_clone,
                        path_clone,
                        skill_content_clone,
                        expanded_ctx,
                        backend_name_clone
                    );
                    
                    // Record the Agentic Pause decision with success/failure outcome
                    self.last_route_decision = Some(RouteDecision::AgenticPause {
                        skill_id: skill_id_for_decision,
                        success: background_success,
                    });
                    
                    if background_success {
                        cluaiz_shared::dev_info!("✅ [Agentic Pause] Dual-Cache `.kvcache.safetensors` safely generated to SSD.");
                        // Only attempt KV load if the cache was saved at a size the main engine can handle.
                        // The background engine used expanded_ctx tokens, but main engine has hardware_n_ctx.
                        if expanded_ctx <= self.hardware_n_ctx {
                            cluaiz_shared::dev_info!("⚙️ [Arbiter] Loading KV cache natively from SSD: {}", cache_path.display());
                            
                            // Unpack safetensors to temporary .temp_state for llama.cpp loading
                            let temp_bin = cache_path.with_extension("temp_state");
                            if let Ok(file) = std::fs::File::open(&cache_path) {
                                if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
                                    if let Ok(st) = safetensors::SafeTensors::deserialize(&mmap) {
                                        if let Some(first_name) = st.names().first() {
                                            if let Ok(tensor) = st.tensor(first_name) {
                                                let _ = std::fs::write(&temp_bin, tensor.data());
                                            }
                                        }
                                    }
                                }
                            }
                            
                            if let Err(e) = self.active_backend.load_kv_cache(&temp_bin.to_string_lossy()) {
                                cluaiz_shared::dev_info!("❌ [Arbiter] Native KV load failed: {}. Force-resetting memory.", e);
                                // 🛡️ Force full memory reset to prevent hybrid SSM/KV state corruption
                                let _ = self.active_backend.prefill("");
                            }
                            
                            let _ = std::fs::remove_file(temp_bin);
                        } else {
                            cluaiz_shared::dev_info!("⚠️ [Arbiter] KV cache was saved at {} ctx but engine has {} ctx. Skipping load (would corrupt hybrid memory).",
                                expanded_ctx, self.hardware_n_ctx);
                        }
                    } else {
                        cluaiz_shared::dev_info!("❌ [Agentic Pause] Hardware failed to acquire background slot. Proceeding safely without skill.");
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
                    cluaiz_shared::dev_info!("⚠️ [ZeroDelayTTFT] Skill ({} chars) exceeds hardware context cap ({} chars). Truncating for safe injection.",
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
                    
                    let backend_name_clone = self.active_backend_name.clone();
                    
                    rt.spawn(async move {
                        let _guard = CompilationGuard { path: cache_path_clone.clone() };
                        use cluaiz_shared::{cluaizContext, StructuralDNA, UnifiedBackend, cluaizInference};
                        let mut temp_dna = StructuralDNA::default();
                        temp_dna.max_context_length = Some(expanded_ctx);
                        let ctx = cluaizContext::boot(temp_dna, cluaiz_shared::TemplateManager::default());
                        
                        cluaiz_shared::dev_info!("🔩 [Arbiter] Asynchronously requesting {} ctx slot in background...", expanded_ctx);
                        let mut booster = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();
                        booster.n_gpu_layers = 0; // Force CPU-only to avoid CUDA device context collisions
                        
                        if let Ok(mut bg_engine) = crate::runtime::execution::hub::HardwareOrchestrator::instantiate_with_booster(
                            &path_clone.to_string_lossy(),
                            &backend_name_clone, 
                            ctx,
                            Some(booster)
                        ).await {
                            let prefill_prompt = if skill_content_clone.starts_with("<|begin_of_text|>") {
                                skill_content_clone.clone()
                            } else {
                                format!(
                                    "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n{}<|eot_id|>\n",
                                    skill_content_clone
                                )
                            };
                            cluaiz_shared::dev_info!("⚙️ [Arbiter] Async background slot acquired. Prefilling {} tokens...", prefill_prompt.len() / 3);
                            if bg_engine.prefill(&prefill_prompt).is_ok() {
                                let bin_path = cache_path_clone.with_extension("temp_state");
                                if bg_engine.dump_kv_cache(&bin_path.to_string_lossy()).is_ok() {
                                    if let Ok(bytes) = std::fs::read(&bin_path) {
                                        if let Ok(view) = safetensors::tensor::TensorView::new(
                                            safetensors::tensor::Dtype::U8, 
                                            vec![bytes.len()], 
                                            &bytes
                                        ) {
                                            let mut metadata = std::collections::HashMap::new();
                                            metadata.insert("model".to_string(), backend_name_clone.clone());
                                            metadata.insert("tokens".to_string(), (prefill_prompt.len() / 3).to_string());
                                            
                                            let state_key = format!("{}_state", backend_name_clone);
                                            if safetensors::serialize_to_file(
                                                vec![(&state_key, view)], 
                                                Some(metadata), 
                                                &cache_path_clone
                                            ).is_ok() {
                                                let _ = std::fs::remove_file(bin_path);
                                                cluaiz_shared::dev_info!("✅ [Arbiter] Async background KV cache compiled and saved as safetensors successfully.");
                                            }
                                        }
                                    }
                                }
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
                    let kv_cache_path = cache_dir.join(format!("{}.kvcache.safetensors", gen_model_safe));
                    if kv_cache_path.exists() {
                        // Check if the skill's token count exceeds what the main engine can actually load
                        let skill_tokens_est = skill.manifest.Core_metadata.as_ref()
                            .map(|m| m.token_count)
                            .unwrap_or(0);
                        if skill_tokens_est > 0 && skill_tokens_est + 256 > self.hardware_n_ctx {
                            cluaiz_shared::dev_info!("⚠️ [Arbiter] Warm cache for '{}' was saved at ~{} tokens but engine has {} ctx. Skipping load.",
                                skill_id, skill_tokens_est + 256, self.hardware_n_ctx);
                            continue;
                        }
                        cluaiz_shared::dev_info!("⚙️ [Arbiter] Warm cache found. Loading KV cache natively from SSD: {}", kv_cache_path.display());
                        // Only override if Agentic Pause didn't already set the decision
                        if self.last_route_decision.is_none() {
                            self.last_route_decision = Some(RouteDecision::WarmCacheHit { skill_id: skill_id.clone() });
                        }
                        
                        let temp_bin = kv_cache_path.with_extension("temp_state");
                        if let Ok(file) = std::fs::File::open(&kv_cache_path) {
                            if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
                                if let Ok(st) = safetensors::SafeTensors::deserialize(&mmap) {
                                    if let Some(first_name) = st.names().first() {
                                        if let Ok(tensor) = st.tensor(first_name) {
                                            let _ = std::fs::write(&temp_bin, tensor.data());
                                        }
                                    }
                                }
                            }
                        }
                        
                        if let Err(e) = self.active_backend.load_kv_cache(&temp_bin.to_string_lossy()) {
                            cluaiz_shared::dev_info!("❌ [Arbiter] Native warm KV load failed: {}. Force-resetting memory.", e);
                            let _ = self.active_backend.prefill("");
                        } else {
                            // Inject the skill description into responses so the prompt prefix matches the KV prefill!
                            if let Some(content) = crate::neural_foundry::extract_skill_body(&skill.path) {
                                intent_result.responses.push(content);
                            } else {
                                intent_result.responses.push(skill.manifest.description.clone());
                            }
                        }
                        
                        let _ = std::fs::remove_file(temp_bin);
                    }
                }
            }
        }
        let max_ctx = self.get_active_dna().and_then(|d| d.max_context_length).unwrap_or(8192);

        match &mut self.active_backend {
            Backend::cluaiz(b) => {

                // If Core signals (skill souls) were identified, inject them into the kernel
                if !intent_result.signals.is_empty() {
                    cluaiz_shared::dev_info!("💉 [Router] Injecting {} M-RoPE KV Cache signals into active hardware...", intent_result.signals.len());
                    b.inject_signals(intent_result.signals).map_err(|e| format!("Signal Injection Failure: {}", e))?;
                }

                let mut formatted_prompt = prompt.to_string();
                
                if !intent_result.responses.is_empty() {
                    let tool_context = intent_result.responses.join("\n\n");
                    if prompt.starts_with("<|begin_of_text|>") {
                        formatted_prompt = prompt.replacen(
                            "<|begin_of_text|>",
                            &format!("<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n{}<|eot_id|>", tool_context),
                            1
                        );
                    } else {
                        formatted_prompt = format!(
                            "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n{}<|eot_id|>\n<|start_header_id|>user<|end_header_id|>\n\n{}<|eot_id|>\n<|start_header_id|>assistant<|end_header_id|>\n\n", 
                            tool_context, 
                            prompt
                        );
                    }
                    cluaiz_shared::dev_info!("🧠 [Router] Injected skill descriptions (Zero-Delay TTFT).");
                }

                let cb = std::sync::Arc::new(std::sync::Mutex::new(callback));
                
                let mut current_prompt = formatted_prompt.clone();
                let mut max_iters = 3; // Max agentic pauses per request
                let mut is_continuation = false; // Track if this is a tool-resume iteration
                
                while max_iters > 0 {
                    max_iters -= 1;
                    
                    // 🎯 CONTINUATION MODE: Tool was executed. Stream final answer DIRECTLY.
                    // Do NOT go through the trigger interceptor — the skill instructions in the
                    // system prompt would cause the model to re-trigger the tool in a loop.
                    // Instead: clear KV state, stream raw tokens straight to dashboard, then break.
                    // If model tries to generate another TRIGGER despite the system override, stop it.
                    if is_continuation {
                        let _ = b.prefill("");
                        let cb_final = cb.clone();
                        let mut trig_acc = String::new();
                        if let Err(e) = b.generate_stream(&current_prompt, max_tokens, Box::new(move |token: String| {
                            trig_acc.push_str(&token);
                            // Stop immediately if model tries to call a tool again
                            if trig_acc.contains("<TRIGGER:") {
                                return false;
                            }
                            // Keep only last 20 chars for sliding trigger detection
                            if trig_acc.len() > 30 {
                                let keep = trig_acc.len().saturating_sub(20);
                                trig_acc = trig_acc[keep..].to_string();
                            }
                            let mut cb_guard = cb_final.lock().unwrap();
                            (*cb_guard)(token)
                        })) {
                            return Err(e.to_string());
                        }
                        break; // Final answer done — exit loop unconditionally
                    }
                    
                    let tool_state = std::sync::Arc::new(std::sync::Mutex::new((false, String::new(), String::new())));
                    let tool_state_clone = tool_state.clone();
                    
                    let buffer_state = std::sync::Arc::new(std::sync::Mutex::new((false, String::new(), String::new())));
                    let buffer_state_clone = buffer_state.clone();
                    
                    let cb_clone = cb.clone();
                    let mut dynamic_triggers: Vec<String> = Vec::new();
                    if let Ok(skill_router) = cluaiz_shared::skills::router::GLOBAL_SKILL_ROUTER.read() {
                        for skill_id in &matched_skill_ids {
                            if let Some(manifest) = skill_router.loaded_manifests.get(skill_id) {
                                if let Some(grammar) = &manifest.triggers.cel_grammar {
                                    dynamic_triggers.push(grammar.clone());
                                }
                            }
                        }
                    }

                    let interceptor = Box::new(move |token: String| {
                        let mut bs = buffer_state_clone.lock().unwrap();
                        let (capture, json_buffer, buffer_cache) = &mut *bs;
                        
                        let mut cb_guard = cb_clone.lock().unwrap();
                        
                        buffer_cache.push_str(&token);

                        // If we are currently capturing a macro (because a dynamic trigger matched partially)
                        if *capture {
                            json_buffer.push_str(&token);
                        } else {
                            // Check if buffer contains any full dynamic trigger
                            let mut found_trigger = None;
                            for dt in &dynamic_triggers {
                                // Extract the prefix before the '->' or '{' (e.g., "use extension::cluaiz-search")
                                let base_trigger = dt.split("->").next().unwrap_or(dt).split('{').next().unwrap_or(dt).trim();
                                if let Some(idx) = buffer_cache.find(base_trigger) {
                                    found_trigger = Some((idx, base_trigger.to_string()));
                                    break;
                                }
                            }

                            if let Some((idx, dt)) = found_trigger {
                                *capture = true;
                                json_buffer.push_str(&buffer_cache[idx..]);
                                let prefix = buffer_cache[..idx].to_string();
                                buffer_cache.clear();
                                if !prefix.is_empty() {
                                    let _ = (*cb_guard)(prefix);
                                }
                            } else {
                                // Check for partial matches
                                let mut partial_match_idx = None;
                                for dt in &dynamic_triggers {
                                    let base_trigger = dt.split("->").next().unwrap_or(dt).split('{').next().unwrap_or(dt).trim();
                                    for (i, _) in buffer_cache.char_indices() {
                                        let suffix = &buffer_cache[i..];
                                        if base_trigger.starts_with(suffix) {
                                            partial_match_idx = Some(i);
                                            break;
                                        }
                                    }
                                    if partial_match_idx.is_some() {
                                        break;
                                    }
                                }
                                
                                // Legacy check for <TRIGGER: just in case it is forced
                                if partial_match_idx.is_none() {
                                    let trigger_target = "<TRIGGER:";
                                    if let Some(idx) = buffer_cache.find(trigger_target) {
                                        *capture = true;
                                        json_buffer.push_str(&buffer_cache[idx..]);
                                        let prefix = buffer_cache[..idx].to_string();
                                        buffer_cache.clear();
                                        if !prefix.is_empty() {
                                            let _ = (*cb_guard)(prefix);
                                        }
                                    } else {
                                        for (i, _) in buffer_cache.char_indices() {
                                            let suffix = &buffer_cache[i..];
                                            if trigger_target.starts_with(suffix) {
                                                partial_match_idx = Some(i);
                                                break;
                                            }
                                        }
                                    }
                                }
                                
                                if let Some(idx) = partial_match_idx {
                                    if idx > 0 {
                                        let text_to_flush = buffer_cache[..idx].to_string();
                                        let new_buffer = buffer_cache[idx..].to_string();
                                        *buffer_cache = new_buffer;
                                        let _ = (*cb_guard)(text_to_flush);
                                    }
                                    return true; // Wait for more tokens
                                } else {
                                    let text = buffer_cache.clone();
                                    buffer_cache.clear();
                                    return (*cb_guard)(text);
                                }
                            }
                        }
                        
                        let is_complete = if json_buffer.starts_with("<TRIGGER:") {
                            json_buffer.contains("</TRIGGER>")
                        } else if json_buffer.contains('{') {
                            json_buffer.contains('}')
                        } else {
                            json_buffer.contains('\n') || json_buffer.contains('>')
                        };
                        
                        if *capture && is_complete {
                            // If it's a Sovereign trigger
                            if json_buffer.starts_with("<TRIGGER:") && json_buffer.contains("</TRIGGER>") {
                                if let Some(trigger_start) = json_buffer.rfind("<TRIGGER:") {
                                    let actual_buffer = &json_buffer[trigger_start..];
                                    if let Some(header_end) = actual_buffer.find('>') {
                                        let header = &actual_buffer[..header_end];
                                        let parts: Vec<&str> = header.trim_start_matches("<TRIGGER:").split(':').collect();
                                        let t_type = if parts.len() >= 2 { parts[0] } else { "extension" };
                                        let t_name = if parts.len() >= 2 { parts[1] } else { parts[0] };
                                        
                                        let json_start = header_end + 1;
                                        let json_end = actual_buffer.find("</TRIGGER>").unwrap_or(actual_buffer.len());
                                        let mut payload = actual_buffer[json_start..json_end].trim().to_string();
                                        payload = payload.trim_end_matches('}').to_string();
                                        if !payload.ends_with('}') {
                                            payload.push('}');
                                        }
                                        
                                        let mut state = tool_state_clone.lock().unwrap();
                                        state.0 = true;
                                        state.1 = format!("{}:{}", t_type, t_name);
                                        state.2 = payload; 
                                    
                                    let _ = (*cb_guard)(format!("⚙️ [Agentic Pause] Engine intercepted tool execution for '{}'...\n", t_name));
                                    return false; // STOP generation
                                }
                                }
                            } else {
                                // Dynamic CEL script execution interceptor!
                                let clean_cel = json_buffer.trim().to_string();
                                
                                let mut state = tool_state_clone.lock().unwrap();
                                state.0 = true;
                                state.1 = "DYNAMIC_CEL".to_string();
                                state.2 = clean_cel.clone(); 
                                
                                let _ = (*cb_guard)(format!("⚙️ [Agentic Pause] Orchestrating CEL macro: {}\n", clean_cel));
                                return false; // STOP generation
                            }
                            
                            // if not valid, flush and reset
                            let flushed = json_buffer.clone();
                            *capture = false;
                            json_buffer.clear();
                            let _ = (*cb_guard)(flushed);
                        }
                        
                        true
                    });
                    
                    if let Err(e) = b.generate_stream(&current_prompt, max_tokens, interceptor) {
                        return Err(e.to_string());
                    }
                    
                    // Fallback for unclosed triggers (if the LLM stopped generating without </TRIGGER>)
                    {
                        let mut bs = buffer_state.lock().unwrap();
                        let capture = bs.0;
                        let json_buffer = &bs.1;
                        if capture && json_buffer.starts_with("<TRIGGER:") && json_buffer.contains('}') {
                            if let Some(trigger_start) = json_buffer.rfind("<TRIGGER:") {
                                let actual_buffer = &json_buffer[trigger_start..];
                                if let Some(header_end) = actual_buffer.find('>') {
                                    let header = &actual_buffer[..header_end];
                                    let parts: Vec<&str> = header.trim_start_matches("<TRIGGER:").split(':').collect();
                                    let t_type = if parts.len() >= 2 { parts[0] } else { "extension" };
                                    let t_name = if parts.len() >= 2 { parts[1] } else { parts[0] };
                                    
                                    let json_start = header_end + 1;
                                    let json_end = actual_buffer.find("</TRIGGER>").unwrap_or(actual_buffer.len());
                                    let mut payload = actual_buffer[json_start..json_end].trim().to_string();
                                    payload = payload.trim_end_matches('}').to_string();
                                    if !payload.ends_with('}') {
                                        payload.push('}');
                                    }
                                    
                                    let mut state = tool_state.lock().unwrap();
                                    state.0 = true;
                                    state.1 = format!("{}:{}", t_type, t_name);
                                    state.2 = payload;
                                    
                                    let mut cb_guard = cb.lock().unwrap();
                                    let _ = (cb_guard)(format!("⚙️ [Agentic Pause] Engine forcefully intercepted unclosed tool execution for '{}'...\n", t_name));
                                }
                            }
                        }
                    }
                    
                    let (was_called, mut t_name, t_payload) = {
                        let state = tool_state.lock().unwrap();
                        (state.0, state.1.clone(), state.2.clone())
                    };
                    
                    if was_called {
                        let mut result_str = String::new();
                        
                        if t_name == "DYNAMIC_CEL" {
                            let clean_cel = t_payload.clone();
                            if let Ok(ast) = inference_cel::parser::lexer::parse(&clean_cel) {
                                let planner = inference_cel::parser::planner::CelPlanner::new();
                                if let Ok(plan) = planner.build_plan(&ast) {
                                    // Internal execute function directly in Router!
                                    let mut exec_result = String::new();
                                    for block in plan.blocks {
                                        match block {
                                            inference_cel::parser::planner::PlanBlock::Pipeline(pipeline_plan) => {
                                                for step in pipeline_plan.steps {
                                                    match step {
                                                        inference_cel::parser::planner::PlanStep::ExecuteAction { method, args } => {
                                                            let executor = crate::neural_foundry::executor::sandbox::UnifiedExecutor::new();
                                                            use inference_cel::ffi::cxp_ffi::{ExtensionPayload, PayloadType, Transpiler};
                                                            let binary_args = Transpiler::to_binary_payload(&args).unwrap_or(vec![]);
                                                            let ext_payload = ExtensionPayload::new(PayloadType::Bincode, &binary_args);
                                                            match executor.execute(&method, &ext_payload) {
                                                                Ok(bytes) => exec_result.push_str(&String::from_utf8_lossy(bytes.as_ref())),
                                                                Err(e) => exec_result.push_str(&format!("[Error] Plugin Execution: {}\n", e)),
                                                            }
                                                        },
                                                        _ => {
                                                            exec_result.push_str("[Engine] Unhandled CEL plan step.\n");
                                                        }
                                                    }
                                                }
                                            },
                                            _ => {
                                                exec_result.push_str("[Engine] Unhandled complex CEL block.\n");
                                            }
                                        }
                                    }
                                    
                                    result_str = exec_result;
                                    let mut cb_guard = cb.lock().unwrap();
                                    let _ = (cb_guard)(format!("✅ [Agentic Pause] Tool output injected into kernel!\n\n"));
                                } else {
                                    result_str = format!("[Error] Failed to build CEL execution plan.");
                                }
                            } else {
                                result_str = format!("[Error] Failed to parse CEL macro AST.");
                            }
                        } else {
                            // Legacy execution path (Sovereign format)
                            let mut t_type = "extension";
                            let mut actual_name = t_name.as_str();
                            if let Some((parsed_type, parsed_name)) = t_name.split_once(':') {
                                t_type = parsed_type;
                                actual_name = parsed_name;
                            }
                            
                            use inference_cel::ffi::cxp_ffi::{ExtensionPayload, PayloadType};
                            let executor = crate::neural_foundry::executor::sandbox::UnifiedExecutor::new(); 
                            let ext_payload = ExtensionPayload::new(PayloadType::Json, t_payload.as_bytes());
                            
                            match executor.execute(actual_name, &ext_payload) {
                                Ok(bytes) => {
                                    result_str = String::from_utf8_lossy(&bytes).to_string();
                                    let mut cb_guard = cb.lock().unwrap();
                                    let _ = (cb_guard)(format!("__ENGINE_PAUSE_EXECUTE__:{}:{}", actual_name, result_str));
                                },
                                Err(e) => {
                                    result_str = format!("Error executing {}: {}", actual_name, e);
                                    let mut cb_guard = cb.lock().unwrap();
                                    let _ = (cb_guard)(format!("__ENGINE_PAUSE_EXECUTE__:{}:{}", actual_name, result_str));
                                }
                            }
                            
                            // Replace t_name with the properly formatted string for the resume tag
                            t_name = format!("{}:{}", t_type, actual_name);
                        }
                        // 🚀 SOVEREIGN KV-CACHE RESUME
                        // We append the tool result to the full prompt history. The native backend will automatically perform prefix-matching KV cache reuse, safely rolling back any hallucinated EOS tokens and resolving subword boundaries natively.
                        let max_chars = max_ctx * 2; // Dynamic estimation
                        let safe_result_str = if result_str.len() > max_chars {
                            format!("{}... [TRUNCATED BY SYSTEM LIMIT]", &result_str[..max_chars])
                        } else {
                            result_str.clone()
                        };
                        
                        let generated_trigger_text = if t_name == "DYNAMIC_CEL" {
                            t_payload.clone()
                        } else { 
                            format!("<TRIGGER:{}>{}</TRIGGER>", t_name, t_payload)
                        };
                        
                        current_prompt.push_str(&generated_trigger_text);
                        // System override message: highest priority in Llama-3 instruct RLHF.
                        // A new system turn mid-conversation overrides the original skill instructions
                        // that tell the model to use tools — preventing re-trigger loops.
                        current_prompt.push_str(&format!(
                            "<|eot_id|>\n<|start_header_id|>system<|end_header_id|>\n\nTool execution is complete. DO NOT call any tools. DO NOT generate any TRIGGER tags. Write your final conversational answer using the search result below.\n<|eot_id|>\n<|start_header_id|>user<|end_header_id|>\n\n<result:{}>\n{}\n</result>\n<|eot_id|>\n<|start_header_id|>assistant<|end_header_id|>\n\n",
                            t_name, safe_result_str
                        ));
                        is_continuation = true; // Next iteration must reset KV cache
                        
                    } else {
                        break; // Normal generation finished without tools
                    }
                }
                
                Ok(())
            },
            Backend::Empty(_) => Err("Core weights not loaded. Please select a model with @ or wait for the Auto-Pilot handshake to complete.".to_string()),
        }
    }
    
    pub fn embed(&mut self, input: &str) -> Result<Vec<f32>, String> {
        self.active_backend.embed(input).map_err(|e| e.to_string())
    }
}

pub struct DummyBackend;
impl cluaiz_shared::UnifiedBackend for DummyBackend {
    fn generate(&mut self, _prompt: &str, _max_tokens: usize) -> Result<String, String> {
        Err("Core weights not loaded.".to_string())
    }
    fn prefill(&mut self, _prompt: &str) -> anyhow::Result<()> { Ok(()) }
    fn evaluate_tps(&self) -> f64 { 0.0 }
}

impl cluaiz_shared::cluaizInference for DummyBackend {
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

pub fn agentic_pause_compile_cache(
    rt: &tokio::runtime::Handle,
    cache_path_clone: std::path::PathBuf,
    path_clone: std::path::PathBuf,
    skill_content_clone: String,
    expanded_ctx: usize,
    backend_name: String,
) -> bool {
    std::thread::scope(|s| {
        s.spawn(|| {
            rt.block_on(async move {
                let _guard = CompilationGuard { path: cache_path_clone.clone() };
                use cluaiz_shared::{cluaizContext, StructuralDNA, UnifiedBackend, cluaizInference};
                let mut temp_dna = StructuralDNA::default();
                temp_dna.max_context_length = Some(expanded_ctx);
                let ctx = cluaizContext::boot(temp_dna, cluaiz_shared::TemplateManager::default());
                
                cluaiz_shared::dev_info!("🔩 [Arbiter] Requesting {} ctx slot in background (CPU fallback mode)...", expanded_ctx);
                let mut booster = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();
                booster.n_gpu_layers = 0; // Force CPU-only to avoid CUDA device context collisions
                
                if let Ok(mut bg_engine) = crate::runtime::execution::hub::HardwareOrchestrator::instantiate_with_booster(
                    &path_clone.to_string_lossy(),
                    &backend_name, 
                    ctx,
                    Some(booster)
                ).await {
                    let prefill_prompt = if skill_content_clone.starts_with("<|begin_of_text|>") {
                        skill_content_clone.clone()
                    } else {
                        format!(
                            "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n{}<|eot_id|>\n",
                            skill_content_clone
                        )
                    };
                    cluaiz_shared::dev_info!("⚙️ [Arbiter] Background slot acquired ({}). Prefilling {} tokens...", backend_name, prefill_prompt.len() / 3);
                    if bg_engine.prefill(&prefill_prompt).is_ok() {
                        let bin_path = cache_path_clone.with_extension("temp_state");
                        if bg_engine.dump_kv_cache(&bin_path.to_string_lossy()).is_ok() {
                            // Wrap bin_path into safetensors and save to cache_path_clone
                            if let Ok(bytes) = std::fs::read(&bin_path) {
                                if let Ok(view) = safetensors::tensor::TensorView::new(
                                    safetensors::tensor::Dtype::U8, 
                                    vec![bytes.len()], 
                                    &bytes
                                ) {
                                    let mut metadata = std::collections::HashMap::new();
                                    metadata.insert("model".to_string(), backend_name.clone());
                                    metadata.insert("tokens".to_string(), (prefill_prompt.len() / 3).to_string());
                                    
                                    let state_key = format!("{}_state", backend_name);
                                    if safetensors::serialize_to_file(
                                        vec![(&state_key, view)], 
                                        Some(metadata), 
                                        &cache_path_clone
                                    ).is_ok() {
                                        let _ = std::fs::remove_file(bin_path);
                                        return true;
                                    }
                                }
                            }
                        } else {
                            cluaiz_shared::dev_info!("⚠️ [Arbiter] Background engine '{}' does not support KV Cache dumping. Skipping.", backend_name);
                            return false;
                        }
                    }
                }
                false
            })
        }).join().unwrap()
    })
}
