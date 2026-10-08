use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SkillMetadata {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default, alias = "allowed-tools")]
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedSkill {
    pub metadata: SkillMetadata,
    pub prompt_instructions: String,
}

pub struct SkillParser;

impl SkillParser {
    /// Parses a `SKILL.md` file, extracting its YAML frontmatter metadata and markdown instruction body
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Option<ParsedSkill> {
        let content = std::fs::read_to_string(path).ok()?;
        Self::parse_content(&content)
    }

    /// Parses raw markdown text containing `---` frontmatter
    pub fn parse_content(content: &str) -> Option<ParsedSkill> {
        let normalized = content.replace("\r\n", "\n");
        if let Some(start) = normalized.find("---\n") {
            if let Some(end) = normalized[start + 4..].find("\n---") {
                let yaml_str = &normalized[start + 4..start + 4 + end];
                let body = normalized[start + 4 + end + 4..].trim().to_string();

                let metadata = if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(yaml_str) {
                    let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let version = val.get("version").or_else(|| val.get("latest_version")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let description = val.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    
                    let mut triggers = Vec::new();
                    if let Some(t_arr) = val.get("triggers").and_then(|t| t.as_sequence()) {
                        triggers = t_arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    } else if let Some(t_obj) = val.get("triggers").or_else(|| val.get("discovery")) {
                        if let Some(arr) = t_obj.get("semantic_triggers").or_else(|| t_obj.get("semantic")).and_then(|s| s.as_sequence()) {
                            triggers = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                        }
                    }

                    let mut permissions = Vec::new();
                    if let Some(p_arr) = val.get("permissions").and_then(|p| p.as_sequence()) {
                        permissions = p_arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    }

                    let mut allowed_tools = Vec::new();
                    if let Some(a_arr) = val.get("allowed-tools").or_else(|| val.get("allowed_tools")).and_then(|a| a.as_sequence()) {
                        allowed_tools = a_arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    }

                    SkillMetadata {
                        name,
                        version,
                        description,
                        triggers,
                        permissions,
                        allowed_tools,
                    }
                } else {
                    SkillMetadata::default()
                };

                return Some(ParsedSkill {
                    metadata,
                    prompt_instructions: body,
                });
            }
        }

        // Fallback if no frontmatter
        Some(ParsedSkill {
            metadata: SkillMetadata::default(),
            prompt_instructions: content.trim().to_string(),
        })
    }
}
