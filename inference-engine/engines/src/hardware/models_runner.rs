//! ═══════════════════════════════════════════════════════════════════════
//!  Hardware: Models Runner (Chat / Persona / Embedding Dispatch)
//! ═══════════════════════════════════════════════════════════════════════

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsRunner {
    pub model_chat: ChatProfile,
    pub model_persona: PersonaProfile,
    #[serde(rename = "model_embedding")]
    pub profile_embedding: EmbeddingProfile,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatProfile {
    pub priority: String,
    pub hardware: String,
    pub kv_cache_bit: u8,
    pub ram_buffer: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PersonaProfile {
    pub hardware: String,
    pub load_balance_ratio: String,
    #[serde(rename = "TurboQuant_mapping")]
    pub turbo_quant_mapping: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingProfile {
    pub hardware: String,
    pub instruction_bypass: String,
    pub ram_buffer: f64,
}

/// ModelCategory: Determines which profile to use based on model folder path
#[derive(Debug, Clone, PartialEq)]
pub enum ModelCategory {
    Chat,
    Persona,
    Embedding,
    Unknown,
}

impl ModelCategory {
    /// Detect category from model path (checks parent folder name)
    pub fn from_path(path: &std::path::Path) -> Self {
        let path_str = path.to_string_lossy().to_lowercase();
        if path_str.contains("/chat/") || path_str.contains("\\chat\\") {
            ModelCategory::Chat
        } else if path_str.contains("/persona/") || path_str.contains("\\persona\\") {
            ModelCategory::Persona
        } else if path_str.contains("/embedding") || path_str.contains("\\embedding") {
            ModelCategory::Embedding
        } else {
            ModelCategory::Unknown
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ModelCategory::Chat => "chat",
            ModelCategory::Persona => "persona",
            ModelCategory::Embedding => "embedding",
            ModelCategory::Unknown => "unknown",
        }
    }
}

impl ModelsRunner {
    /// Select the correct backend suffix based on model category and hardware profile
    pub fn select_backend_suffix(&self, category: &ModelCategory, gpu_available: bool) -> String {
        let hw_target = match category {
            ModelCategory::Chat => &self.model_chat.hardware,
            ModelCategory::Persona => &self.model_persona.hardware,
            ModelCategory::Embedding => &self.profile_embedding.hardware,
            ModelCategory::Unknown => "Auto",
        };

        match &hw_target[..] {
            "GPU_Only" => {
                if gpu_available {
                    "cuda".to_string() // Future: detect vendor (cuda/metal)
                } else {
                    tracing::warn!("⚠️ GPU_Only requested but no GPU — falling back to CPU");
                    "cpu".to_string()
                }
            }
            "CPU_Optimized" => "cpu".to_string(),
            "Hybrid_CPU_GPU" | "Auto" | _ => {
                if gpu_available {
                    "vulkan".to_string() // Default to Vulkan for cross-platform GPU
                } else {
                    "cpu".to_string()
                }
            }
        }
    }

    /// Parse the load balance ratio (e.g., "70:30" → GPU gets 70%, CPU gets 30%)
    pub fn persona_gpu_ratio(&self) -> (f64, f64) {
        let parts: Vec<&str> = self.model_persona.load_balance_ratio.split(':').collect();
        if parts.len() == 2 {
            let gpu = parts[0].trim().parse::<f64>().unwrap_or(70.0);
            let cpu = parts[1].trim().parse::<f64>().unwrap_or(30.0);
            (gpu / 100.0, cpu / 100.0)
        } else {
            (0.7, 0.3)
        }
    }

    /// Get RAM buffer for a specific model category
    pub fn ram_buffer_gb(&self, category: &ModelCategory) -> f64 {
        match category {
            ModelCategory::Chat => self.model_chat.ram_buffer,
            ModelCategory::Embedding => self.profile_embedding.ram_buffer,
            _ => 1.0, // Default 1GB buffer
        }
    }

    pub fn log_dispatch(&self, category: &ModelCategory) {
        match category {
            ModelCategory::Chat => tracing::info!(
                "🗨️ Chat Model: {} | Priority: {} | KV: {}bit | Buffer: {}GB",
                self.model_chat.hardware, self.model_chat.priority,
                self.model_chat.kv_cache_bit, self.model_chat.ram_buffer
            ),
            ModelCategory::Persona => tracing::info!(
                "🧠 Persona Model: {} | Balance: {} | TurboQuant: {}",
                self.model_persona.hardware, self.model_persona.load_balance_ratio,
                self.model_persona.turbo_quant_mapping
            ),
            ModelCategory::Embedding => tracing::info!(
                "📊 Embedding Model: {} | AVX: {} | Buffer: {}GB",
                self.profile_embedding.hardware, self.profile_embedding.instruction_bypass,
                self.profile_embedding.ram_buffer
            ),
            ModelCategory::Unknown => tracing::info!("❓ Unknown model category — using Auto dispatch"),
        }
    }
}
