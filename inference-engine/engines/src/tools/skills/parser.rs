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
    pub author: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
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

                let metadata: SkillMetadata = serde_yaml::from_str(yaml_str).unwrap_or_default();
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
