use crate::neural_foundry::registry::SkillManifest;
use std::path::Path;

pub struct SkillParser;

impl SkillParser {
    /// Parses a SkillManifest from a file.
    /// If the file is `SKILL.md`, it extracts the YAML frontmatter.
    /// Otherwise, it assumes JSON format.
    pub fn parse<P: AsRef<Path>>(manifest_path: P, content: &str) -> Option<SkillManifest> {
        let file_name = manifest_path.as_ref().file_name().and_then(|n| n.to_str()).unwrap_or("");
        
        if file_name == "SKILL.md" || file_name == "skill.md" {
            Self::parse_frontmatter(content)
        } else if file_name.ends_with(".yaml") || file_name.ends_with(".yml") {
            serde_yaml::from_str::<SkillManifest>(content).ok()
        } else {
            serde_json::from_str::<SkillManifest>(content).ok()
        }
    }

    fn parse_frontmatter(content: &str) -> Option<SkillManifest> {
        let normalized = content.replace("\r\n", "\n");
        if let Some(start) = normalized.find("---\n") {
            if let Some(end) = normalized[start + 4..].find("\n---") {
                let yaml_content = &normalized[start + 4..start + 4 + end];
                return serde_yaml::from_str::<SkillManifest>(yaml_content).ok();
            }
        }
        None
    }
}
