use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IntegrationMetadata {
    pub name: String,
    pub version: String,
    pub compatibility: Option<Vec<String>>,
    pub permissions: Option<HashMap<String, bool>>,
    pub semantic_triggers: Option<Vec<String>>,
    /// Dynamically maps logical assets (e.g. "logic", "state") to relative file paths.
    pub links: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Integration {
    pub metadata: IntegrationMetadata,
    pub instructions: String,
    /// Absolute paths to all resolved assets defined in the metadata `links`.
    pub resolved_links: HashMap<String, PathBuf>,
}

pub struct MetadataParser;

impl MetadataParser {
    /// Parses any Markdown integration file (e.g., `SKILL.md`, `PLUGIN.md`, `SOUL.md`).
    /// It automatically caches the parsed structure to a `.bin` file.
    pub fn parse_file(path: &Path) -> Result<Integration, String> {
        let bin_path = path.with_extension("bin");

        // 1. FAST PATH: Binary Cache
        if bin_path.exists() {
            let bin_data = fs::read(&bin_path).map_err(|e| format!("Failed to read .bin cache: {}", e))?;
            if let Ok(integration) = bincode::deserialize(&bin_data) {
                tracing::info!("🚀 Loaded Integration from Binary Cache: {:?}", bin_path);
                return Ok(integration);
            }
        }

        // 2. SLOW PATH: Parse Markdown Text
        tracing::info!("📝 Parsing Integration YAML from text for {:?}", path);
        let content = fs::read_to_string(path).map_err(|e| format!("Failed to read Markdown file: {}", e))?;
        
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err("Invalid format. Expected YAML frontmatter bounded by '---'.".to_string());
        }

        let yaml_str = parts[1];
        let instructions = parts[2].trim().to_string();

        let metadata: IntegrationMetadata = serde_yaml::from_str(yaml_str)
            .map_err(|e| format!("Failed to parse YAML frontmatter: {}", e))?;
            
        // 3. Resolve Asset Links dynamically
        let parent_dir = path.parent().unwrap_or(Path::new(""));
        let mut resolved_links = HashMap::new();

        if let Some(links) = &metadata.links {
            for (key, relative_path) in links {
                // If it's a file, resolve the absolute path
                let absolute_path = parent_dir.join(relative_path);
                if absolute_path.exists() && absolute_path.is_file() {
                    resolved_links.insert(key.clone(), absolute_path);
                }
            }
        }

        let integration = Integration {
            metadata,
            instructions,
            resolved_links,
        };

        // 4. Compile to Binary Cache
        if let Ok(bin_data) = bincode::serialize(&integration) {
            let _ = fs::write(&bin_path, bin_data);
        }

        Ok(integration)
    }
}
