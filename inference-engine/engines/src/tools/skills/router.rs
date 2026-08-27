use std::collections::HashMap;
use std::path::PathBuf;
use cluaiz_shared::environment::EnvironmentManager;
use crate::tools::registry::ToolsRegistry;
use super::parser::{SkillParser, ParsedSkill};

/// O(1) Keyword and Semantic Trigger Router for Skills
pub struct SkillRouter {
    /// Maps keyword trigger (lowercase) -> skill ID
    keyword_map: HashMap<String, String>,
    /// Cache of parsed skills: skill ID -> ParsedSkill
    parsed_cache: HashMap<String, ParsedSkill>,
}

impl Default for SkillRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRouter {
    pub fn new() -> Self {
        let mut router = Self {
            keyword_map: HashMap::new(),
            parsed_cache: HashMap::new(),
        };
        let _ = router.rebuild_index();
        router
    }

    /// Rebuilds keyword trigger map from active skills in ToolsRegistry
    pub fn rebuild_index(&mut self) -> anyhow::Result<()> {
        self.keyword_map.clear();
        self.parsed_cache.clear();

        let registry = ToolsRegistry::load()?;
        let env = EnvironmentManager::current();

        for (id, entry) in &registry.installed_tools {
            if entry.category == "skill" && entry.enabled {
                let skill_dir = PathBuf::from(&entry.local_dir);
                let skill_md = skill_dir.join("SKILL.md");
                if skill_md.exists() {
                    if let Some(parsed) = SkillParser::parse_file(&skill_md) {
                        self.parsed_cache.insert(id.clone(), parsed);
                    }
                }

                for trigger in &entry.semantic_triggers {
                    let normalized = trigger.to_lowercase().trim().to_string();
                    if !normalized.is_empty() {
                        self.keyword_map.insert(normalized, id.clone());
                    }
                }
            }
        }
        Ok(())
    }

    /// Matches a user query against known triggers and returns matching skill IDs
    pub fn match_query(&self, user_query: &str) -> Vec<String> {
        let query_lower = user_query.to_lowercase();
        let mut matched = Vec::new();

        for (trigger, skill_id) in &self.keyword_map {
            if query_lower.contains(trigger) {
                if !matched.contains(skill_id) {
                    matched.push(skill_id.clone());
                }
            }
        }
        matched
    }

    /// Returns prompt instructions for a given skill ID
    pub fn get_instructions(&self, skill_id: &str) -> Option<&str> {
        self.parsed_cache.get(skill_id).map(|s| s.prompt_instructions.as_str())
    }
}
