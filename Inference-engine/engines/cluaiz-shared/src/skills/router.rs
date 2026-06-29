use color_eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;
use std::sync::{LazyLock, RwLock};

pub static GLOBAL_SKILL_ROUTER: LazyLock<RwLock<SkillRouter>> = LazyLock::new(|| {
    let mut router = SkillRouter::new();
    let _ = router.boot_index(); // Ignore failure on boot
    RwLock::new(router)
});

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillManifest {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub triggers: SkillTriggers,
    #[serde(default)]
    pub soul_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillTriggers {
    pub semantic: Vec<String>,
    pub entropy_threshold: Option<f32>,
}

/// The Tier 1: Skill Router
/// Lazy loads manifest metadata in O(1) without touching WASM or KV-cache.
pub struct SkillRouter {
    /// Maps a keyword trigger directly to the skill's absolute path
    pub keyword_index: HashMap<String, PathBuf>,
    pub loaded_manifests: HashMap<String, SkillManifest>,
    /// Maps a skill's absolute path to its semantic vector
    pub skill_vectors: HashMap<PathBuf, Vec<f32>>,
    /// Maps a skill's absolute path to its entropy threshold
    pub skill_thresholds: HashMap<PathBuf, f32>,
}

#[derive(serde::Deserialize)]
struct MinimalModelSelection {
    text: Option<String>,
}

#[derive(serde::Deserialize)]
struct MinimalPermissionSchema {
    vector_models: Option<MinimalModelSelection>,
}

fn get_active_embedding_model() -> Option<String> {
    let permission_path = crate::environment::EnvironmentManager::current().config_dir().join("Permission.json");
    if permission_path.exists() {
        if let Ok(content) = fs::read_to_string(permission_path) {
            if let Ok(schema) = serde_json::from_str::<MinimalPermissionSchema>(&content) {
                if let Some(vector_models) = schema.vector_models {
                    return vector_models.text;
                }
            }
        }
    }
    None
}

impl SkillRouter {
    pub fn new() -> Self {
        Self {
            keyword_index: HashMap::new(),
            loaded_manifests: HashMap::new(),
            skill_vectors: HashMap::new(),
            skill_thresholds: HashMap::new(),
        }
    }

    /// Scans the ~/.cluaiz/skills/ directory and builds the FST/Trie index
    pub fn boot_index(&mut self) -> Result<()> {
        let skills_dir = crate::environment::EnvironmentManager::current()
            .ensure_skills_dir()
            .unwrap_or_else(|_| crate::environment::EnvironmentManager::current().skills_dir());
        
        if !skills_dir.exists() {
            return Ok(());
        }

        let active_model = get_active_embedding_model();
        let target_filename = active_model.map(|m| format!("{}.emb.bin", m.replace(":", "-")));

        for entry in fs::read_dir(skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                let manifest_path = path.join("manifest.json");
                let skill_md_path = path.join("SKILL.md");
                
                let parsed_manifest = if manifest_path.exists() {
                    if let Ok(content) = fs::read_to_string(&manifest_path) {
                        serde_json::from_str::<SkillManifest>(&content).ok()
                    } else { None }
                } else if skill_md_path.exists() {
                    if let Ok(content) = fs::read_to_string(&skill_md_path) {
                        let normalized = content.replace("\r\n", "\n");
                        if let Some(start) = normalized.find("---\n") {
                            if let Some(end) = normalized[start + 4..].find("\n---") {
                                let yaml_content = &normalized[start + 4..start + 4 + end];
                                serde_yaml::from_str::<SkillManifest>(yaml_content).ok()
                            } else { None }
                        } else { None }
                    } else { None }
                } else { None };

                if let Some(mut manifest) = parsed_manifest {
                    if manifest.id.is_empty() {
                        manifest.id = manifest.name.clone();
                    }
                    let norm_path = normalize_path(&path);
                    // Register into memory index
                    for keyword in &manifest.triggers.semantic {
                        self.keyword_index.insert(keyword.to_lowercase(), norm_path.clone());
                    }
                    if let Some(thresh) = manifest.triggers.entropy_threshold {
                        self.skill_thresholds.insert(norm_path.clone(), thresh);
                    }
                    self.loaded_manifests.insert(manifest.id.clone(), manifest);
                    
                    // Load Semantic Vector
                    let cache_dir = path.join(".cache");
                    if cache_dir.exists() {
                        if let Some(ref filename) = target_filename {
                            let emb_path = cache_dir.join(filename);
                            if emb_path.exists() {
                                if let Ok(bytes) = fs::read(&emb_path) {
                                    let floats: Vec<f32> = bytes
                                        .chunks_exact(4)
                                        .map(|b| f32::from_ne_bytes(b.try_into().unwrap()))
                                        .collect();
                                    self.skill_vectors.insert(norm_path.clone(), floats);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// O(1) Check if a generated token triggers any skill
    pub fn check_trigger(&self, token_text: &str) -> Option<PathBuf> {
        let normalized = token_text.trim().to_lowercase();
        self.keyword_index.get(&normalized).cloned()
    }

    /// O(N) Check if a prompt vector triggers any skill via Cosine Similarity
    pub fn check_semantic_trigger(&self, prompt_vector: &[f32], default_threshold: f32) -> Option<PathBuf> {
        let mut best_match = None;
        let mut highest_score = -1.0;

        let dim = prompt_vector.len();
        tracing::debug!("[Semantic Check] Prompt vector dim: {}, Skill vectors count: {}", dim, self.skill_vectors.len());

        for (path, skill_vec) in &self.skill_vectors {
            tracing::debug!("[Semantic Check] Checking path: {}, skill_vec len: {}", path.display(), skill_vec.len());
            if !skill_vec.is_empty() && skill_vec.len() % dim == 0 {
                let threshold = self.skill_thresholds.get(path).copied().unwrap_or(default_threshold);
                tracing::debug!("[Semantic Check] Threshold for {}: {}", path.display(), threshold);

                for (idx, chunk) in skill_vec.chunks_exact(dim).enumerate() {
                    let mut dot = 0.0;
                    let mut mag_a = 0.0;
                    let mut mag_b = 0.0;
                    
                    for (a, b) in prompt_vector.iter().zip(chunk.iter()) {
                        dot += a * b;
                        mag_a += a * a;
                        mag_b += b * b;
                    }
                    
                    if mag_a > 0.0 && mag_b > 0.0 {
                        let score = dot / (mag_a.sqrt() * mag_b.sqrt());
                        tracing::debug!("[Semantic Check]   Chunk {} score: {:.4} (Threshold: {:.4})", idx, score, threshold);
                        if score >= threshold && score > highest_score {
                            highest_score = score;
                            best_match = Some(path.clone());
                        }
                    }
                }
            } else {
                tracing::debug!("[Semantic Check]   Skipping check: skill_vec len is not compatible or empty");
            }
        }
        
        if let Some(ref matched_path) = best_match {
            tracing::info!("[Semantic Check] MATCHED: {} with score {:.4}", matched_path.display(), highest_score);
        } else {
            tracing::debug!("[Semantic Check] NO MATCH FOUND");
        }
        best_match
    }
}

pub fn normalize_path(path: &std::path::Path) -> PathBuf {
    let s = path.to_string_lossy().replace("\\", "/").to_lowercase();
    PathBuf::from(s)
}
