//! ═══════════════════════════════════════════════════════════════════════
//!   Engine: The Core Roster (Cluaiz Installation Registry)
//! ═══════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;
use tracing::info;
pub mod provisioner;
pub mod discovery;
pub use provisioner::Provisioner;
pub use discovery::AutonomousDiscovery;
use crate::hardware::SiliconTruth;
use crate::models::fetch::ModelDownloader;
pub use cluaiz_shared::{KernelSignature, StructuralDNA};
use reqwest;

// ─── Installation JSON Schema ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAsset {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationModel {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub architecture: String,
    #[serde(default)]
    pub parameters: String,
    #[serde(default)]
    pub training_tokens: String,
    #[serde(default = "default_bit_depth", deserialize_with = "deserialize_bit_depth")]
    pub bit_depth: f64,
    pub ram_required_gb: f64,
    #[serde(default)]
    pub download_size_gb: f64,
    pub huggingface_repo: String,
    pub download_url: String, // Flat direct link
    #[serde(default)]
    pub description: String,
    pub is_cloud_api: bool,
    #[serde(default)]
    pub requires_gpu: bool,
    #[serde(default = "default_context")]
    pub context_window: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub assets: Vec<ModelAsset>,
    
    // 🔗 AGNOSTIC CAPABILITIES (The DNA Layer)
    #[serde(default)]
    pub has_vision: bool,
    #[serde(default)]
    pub has_audio: bool,
    #[serde(default)]
    pub expert_count: Option<usize>,      // For MoE Models
    #[serde(default)]
    pub experts_per_token: Option<usize>, // For MoE Selection
}

#[derive(Debug, Deserialize)]
struct InstallationFile {
    pub models: Vec<InstallationModel>,
}

fn default_bit_depth() -> f64 { 4.0 }

fn deserialize_bit_depth<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BitDepthVisitor;

    impl<'de> serde::de::Visitor<'de> for BitDepthVisitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a float or a string containing a float")
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value as f64)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(value as f64)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            value.parse::<f64>().map_err(serde::de::Error::custom)
        }
    }

    deserializer.deserialize_any(BitDepthVisitor)
}

fn default_context() -> String { "8k".to_string() }
fn default_category() -> String { "chat".to_string() }


// ─── Public ModelManifest (used by TUI) ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub id: String,
    pub name: String,
    pub architecture: String,
    #[serde(default)]
    pub architecture_type: String,
    pub parameters: String,
    pub training_tokens: String,
    #[serde(default = "default_bit_depth", deserialize_with = "deserialize_bit_depth")]
    pub bit_depth: f64,
    pub ram_required_gb: f64,
    pub download_size_gb: f64,
    pub huggingface_repo: String,
    pub huggingface_filename: String,
    pub download_url: String,
    pub description: String,
    pub is_cloud_api: bool,
    pub requires_gpu: bool,
    #[serde(default)]
    pub is_free_tier: bool,
    pub input_modality: String,
    pub context_window: String,
    pub family: String,
    pub category: String,
    pub assets: Vec<ModelAsset>,
    #[serde(default)]
    pub local_path: Option<String>,
    #[serde(default)]
    pub dna_path: Option<String>,
    
    // 🏛️ DNA CAPABILITIES
    #[serde(default)]
    pub has_vision: bool,
    #[serde(default)]
    pub has_audio: bool,
    pub expert_count: Option<usize>,
    pub experts_per_token: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RosterFile {
    pub models: Vec<ModelManifest>,
}

#[derive(Debug, Serialize)]
pub struct ModelRecommendation {
    pub manifest: ModelManifest,
    pub status: String,
    pub is_cached: bool,
}

// ─── CoreRoster ─────────────────────────────────────────────────────────

pub struct CoreRoster;

pub const REGISTRY_URL: &str = "https://cdn.jsdelivr.net/gh/cluaiz/cluaiz@main/models/library/registry.json";

impl CoreRoster {
    /// 🌐 Fetches an external models.json registry from a URL (Default: jsDelivr).
    pub async fn fetch_external_registry(url: Option<&str>) -> Result<Vec<ModelManifest>, String> {
        let fetch_url = url.unwrap_or(REGISTRY_URL);
        let client = reqwest::Client::builder()
            .user_agent("Cluaiz/1.0")
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;

        let response = client.get(fetch_url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Registry fetch failed: HTTP {}", response.status()));
        }

        let json_val: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        let manifests = Vec::new();

        if let Some(routing) = json_val.get("routing").and_then(|r| r.as_object()) {
            for (_id, _path) in routing {
                // For the index, we might just return empty manifests or placeholders
                // but ideally, the remote should also point to the full JSONs.
                // For now, let's just parse what we can.
            }
        }
        
        Ok(manifests)
    }

    /// Scans the local Cluaiz Library recursively and merges with the Sovereign Registry.
    pub fn load_roster() -> Vec<ModelManifest> {
        let mut registry = std::collections::HashMap::new();

        // 1. Load Autonomous Local Units (The Index Master)
        let model_paths = vec![
            cluaiz_shared::environment::EnvironmentManager::current().models_dir().to_string_lossy().to_string()
        ];

        for path in model_paths {
            let base = Path::new(&path);
            if base.exists() && base.is_dir() {
                info!("🔍 [Roster] Scanning unit path: {}", path);
                let local_units = AutonomousDiscovery::index_Cluaiz_models(base);
                for unit in local_units {
                    info!("✅ [Roster] Discovered local unit: {}", unit.id);
                    registry.insert(unit.id.to_lowercase(), unit);
                }
            }
        }

        // 2. Load Cluaiz Library (The Sovereign Source)
        let mut templates = Vec::new();
        let search_paths = vec![
            "models/library",
            "../models/library",
            "../../models/library",
            "Cluaiz-engine/models/library",
            "../Cluaiz-engine/models/library"
        ];
        
        let mut base_dir = None;
        for p in search_paths {
            let candidate = std::path::PathBuf::from(p);
            if candidate.exists() && candidate.is_dir() { 
                info!("📚 [Roster] Library source found at: {}", p);
                base_dir = Some(candidate); 
                break; 
            }
        }

        if let Some(base) = base_dir {
            // 🔍 RECURSIVE SURGICAL SCAN: Family -> Version -> JSON
            if let Ok(families) = fs::read_dir(&base) {
                for family_entry in families.flatten() {
                    let family_path = family_entry.path();
                    if !family_path.is_dir() { continue; }
                    let family_name = family_path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown").to_string();

                    // Read family.json to get architecture_type
                    let mut arch_type = "gguf".to_string();
                    let family_json_path = family_path.join("family.json");
                    if family_json_path.exists() {
                        if let Ok(content) = fs::read_to_string(&family_json_path) {
                            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&content) {
                                if let Some(a_type) = json_val.get("architecture_type").and_then(|v| v.as_str()) {
                                    arch_type = a_type.to_string();
                                }
                            }
                        }
                    }

                    if let Ok(versions) = fs::read_dir(&family_path) {
                        for version_entry in versions.flatten() {
                            let version_path = version_entry.path();
                            if version_path.file_name() == Some(std::ffi::OsStr::new("family.json")) { continue; }
                            
                            if version_path.is_dir() {
                                if let Ok(files) = fs::read_dir(&version_path) {
                                    for file_entry in files.flatten() {
                                        let json_path = file_entry.path();
                                        if json_path.extension().and_then(|e| e.to_str()) == Some("json") {
                                            if let Ok(batch) = Self::load_installation_file(&json_path, &family_name, &arch_type) {
                                                templates.extend(batch);
                                            }
                                        }
                                    }
                                }
                            } else if version_path.extension().and_then(|e| e.to_str()) == Some("json") {
                                if let Ok(batch) = Self::load_installation_file(&version_path, &family_name, &arch_type) {
                                    templates.extend(batch);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Merge: Local Units take precedence over online templates
        for template in templates {
            let id_key = template.id.to_lowercase();
            if !registry.contains_key(&id_key) {
                registry.insert(id_key, template);
            }
        }

        let mut final_roster: Vec<_> = registry.into_values().collect();
        final_roster.sort_by(|a, b| a.name.cmp(&b.name));
        final_roster
    }

    fn load_installation_file(path: &Path, family_name: &str, arch_type: &str) -> Result<Vec<ModelManifest>, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let json_val: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
            // println!("❌ [Roster] Failed to parse {}: {}", path.display(), e);
            e.to_string()
        })?;
        
        let mut manifests = Vec::new();
        
        if let Some(obj) = json_val.as_object() {
            for (key, value) in obj {
                // 🛑 Skip Meta-Keys
                if key == "family" || key == "version" || key == "registry_name" {
                    continue;
                }

                // 🧬 Each Key is an ID, Value is an Object representing the Model
                if let Some(model_obj) = value.as_object() {
                    let name = model_obj.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
                    let architecture = model_obj.get("architecture").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
                    let context_window = model_obj.get("context_window").and_then(|v| v.as_str()).unwrap_or("8k").to_string();
                    let category = model_obj.get("category").and_then(|v| v.as_str()).unwrap_or("chat").to_string();
                    let default_quant = model_obj.get("default_quant").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let description = model_obj.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    
                    if let Some(variants) = model_obj.get("variants").and_then(|v| v.as_object()) {
                        for (format_key, format_val) in variants {
                            if let Some(quants) = format_val.as_object() {
                                for (quant_key, quant_val) in quants {
                                    if let Some(q_obj) = quant_val.as_object() {
                                        let download_url = q_obj.get("download_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        let ram_required_gb = q_obj.get("ram_required_gb").and_then(|v| {
                                            if v.is_f64() { v.as_f64() } else if v.is_i64() { Some(v.as_i64().unwrap() as f64) } else { None }
                                        }).unwrap_or(0.0);
                                        let download_size_gb = q_obj.get("download_size_gb").and_then(|v| {
                                            if v.is_f64() { v.as_f64() } else if v.is_i64() { Some(v.as_i64().unwrap() as f64) } else { None }
                                        }).unwrap_or(0.0);
                                        
                                        // 💉 INJECT ID from Key + Format + Quant (The Sovereign Way)
                                        let full_id = format!("{}:{}:{}", key, format_key, quant_key);

                                        let gguf_filename = download_url
                                            .split('/')
                                            .next_back()
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| format!("{}-{}.{}", key, quant_key, format_key));

                                        let input_modality = if context_window.contains("256k") || context_window.contains("128k") {
                                            "Text + Vision".to_string()
                                        } else {
                                            "Text".to_string()
                                        };

                                        let m = ModelManifest {
                                            id: full_id,
                                            name: name.clone(),
                                            architecture: architecture.clone(),
                                            architecture_type: arch_type.to_string(),
                                            parameters: "".to_string(), // can be deduced if needed
                                            training_tokens: "".to_string(),
                                            bit_depth: 4.0, // default approximation
                                            ram_required_gb,
                                            download_size_gb,
                                            huggingface_repo: "".to_string(),
                                            huggingface_filename: gguf_filename,
                                            download_url: download_url.clone(),
                                            description: description.clone(),
                                            is_cloud_api: false,
                                            requires_gpu: format_key != "gguf" && format_key != "bitnet",
                                            is_free_tier: true,
                                            input_modality,
                                            context_window: context_window.clone(),
                                            family: family_name.to_string(),
                                            category: category.clone(),
                                            assets: Vec::new(),
                                            local_path: None,
                                            dna_path: None,
                                            has_vision: category == "multimodal" || architecture.contains("VL") || category == "embedding",
                                            has_audio: category == "audio",
                                            expert_count: None,
                                            experts_per_token: None,
                                        };

                                        // Also add base alias for default_quant
                                        if quant_key == &default_quant && format_key == "gguf" {
                                            let mut alias = m.clone();
                                            alias.id = key.clone();
                                            manifests.push(alias);
                                        }

                                        manifests.push(m);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(manifests)
    }

    /// Takes the hardware spec and returns the available models with recommendations.
    pub fn get_recommendations(hardware: &SiliconTruth, system_ram_gb: f64) -> Vec<ModelRecommendation> {
        let models = Self::load_roster();
        let mut recommendations = Vec::new();

        let system_ram_gb = if system_ram_gb > 0.0 { system_ram_gb } else { 1.0 };
        let usable_ram = system_ram_gb * 0.95;

        for mut model in models {
            let cached_path = if model.is_cloud_api { None } else {
                ModelDownloader::get_cached_path(&model.category, &model.id, &model.huggingface_filename)
            };
            
            let is_cached = model.is_cloud_api || cached_path.is_some();
            if let Some(path) = cached_path {
                model.local_path = Some(path.to_string_lossy().to_string());
            }

            let status = if model.is_cloud_api {
                "Cloud".to_string()
            } else if model.requires_gpu {
                if hardware.accelerators.gpus.is_empty() {
                    "Incompatible".to_string()
                } else {
                    let vram_gb = hardware.accelerators.gpus.iter().map(|g| g.vram_available_gb).sum::<f64>();
                    if model.ram_required_gb > vram_gb {
                        "Incompatible".to_string()
                    } else {
                        let gap = (vram_gb - model.ram_required_gb) / vram_gb;
                        if gap > 0.25 { "Optimal".to_string() }
                        else if gap >= 0.10 { "Average".to_string() }
                        else { "Heavy".to_string() }
                    }
                }
            } else {
                let gap = (usable_ram - model.ram_required_gb) / usable_ram;
                if gap > 0.25 { "Optimal".to_string() }
                else if gap >= 0.10 { "Average".to_string() }
                else if gap >= 0.0 { "Heavy".to_string() }
                else { "Impossible".to_string() }
            };

            info!("🧬 [Roster] Model {} Status: {}, Cached: {}", model.id, status, is_cached);
            recommendations.push(ModelRecommendation { manifest: model, status, is_cached });
        }

        recommendations.sort_by(|a, b| {
            let score = |s: &str| match s {
                "Optimal" => 0, "Average" => 1, "Heavy" => 2,
                "Cloud" => 3, "Incompatible" => 4, "Impossible" => 5, _ => 6,
            };
            score(&a.status).cmp(&score(&b.status))
        });

        info!("📊 [Roster] Generated {} model recommendations for UI.", recommendations.len());
        recommendations
    }
}
