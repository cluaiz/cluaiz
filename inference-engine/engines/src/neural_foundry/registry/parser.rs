use crate::neural_foundry::registry::{SkillManifest, Triggers, Permissions};
use std::path::Path;

pub struct SkillParser;

impl SkillParser {
    /// Parses a SkillManifest from a file.
    /// If the file is `SKILL.md`, it extracts the YAML frontmatter.
    /// Otherwise, it assumes JSON format.
    pub fn parse<P: AsRef<Path>>(manifest_path: P, content: &str) -> Option<SkillManifest> {
        let file_name = manifest_path.as_ref().file_name().and_then(|n| n.to_str()).unwrap_or("");
        
        let parsed = if file_name == "SKILL.md" || file_name == "skill.md" {
            Self::parse_frontmatter(content)
        } else if file_name.ends_with(".yaml") || file_name.ends_with(".yml") {
            if let Ok(manifest) = serde_yaml::from_str::<SkillManifest>(content) {
                Some(manifest)
            } else if let Ok(integration) = serde_yaml::from_str::<inference_cel::parser::metadata_parser::IntegrationMetadata>(content) {
                // Adapt IntegrationMetadata to SkillManifest
                let semantic = integration.discovery.as_ref().and_then(|d| d.semantic_triggers.clone()).unwrap_or_default();
                let cel_grammar = integration.discovery.as_ref().and_then(|d| d.cel_grammar.clone());
                let triggers = Triggers {
                    semantic,
                    entropy_threshold: None,
                    hard_trigger_tokens: Vec::new(),
                    cooldown_on_failure_tokens: 0,
                };
                let permissions = Permissions {
                    level: "ReadOnly".to_string(),
                    network: integration.permissions.as_ref().and_then(|p| p.network_access).unwrap_or(false),
                    filesystem: integration.permissions.as_ref().map(|p| p.file_system.is_some()).unwrap_or(false),
                    mcp_servers: Vec::new(),
                };
                Some(SkillManifest {
                    id: integration.name.clone(),
                    name: integration.name,
                    title: String::new(),
                    version: integration.version,
                    author: String::new(),
                    description: integration.description.unwrap_or_default(),
                    keywords: Vec::new(),
                    triggers,
                    permissions,
                    computational_budget: None,
                    user_profile_binding: None,
                    soul_type: "extension".to_string(),
                    Core_metadata: None,
                })
            } else {
                None
            }
        } else {
            serde_json::from_str::<SkillManifest>(content).ok()
        };

        parsed
    }

    fn parse_frontmatter(content: &str) -> Option<SkillManifest> {
        let normalized = content.replace("\r\n", "\n");
        if let Some(start) = normalized.find("---\n") {
            if let Some(end) = normalized[start + 4..].find("\n---") {
                let yaml_content = &normalized[start + 4..start + 4 + end];
                return Self::parse(Path::new("SKILL.yaml"), yaml_content);
            }
        }
        None
    }
}
