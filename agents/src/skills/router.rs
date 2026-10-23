use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
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

/// The router built for the current registry generation (parsing every SKILL.md once per
/// registry change instead of on every lookup)
static SHARED: LazyLock<Mutex<Option<(u64, Arc<SkillRouter>)>>> = LazyLock::new(|| Mutex::new(None));

impl SkillRouter {
    /// The router for the registry as it is now, rebuilt only when the registry changed
    pub fn shared() -> Arc<SkillRouter> {
        let generation = ToolsRegistry::generation();
        let mut guard = SHARED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((built_for, router)) = guard.as_ref() {
            if *built_for == generation {
                return router.clone();
            }
        }
        let router = Arc::new(Self::new());
        *guard = Some((generation, router.clone()));
        router
    }

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
        let mut matched = Vec::new();

        // Whole words only: the trigger "rust" must not match "trust"
        for (trigger, skill_id) in &self.keyword_map {
            if crate::tool_search::contains_phrase(user_query, trigger) {
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

    /// Returns allowed tools restriction for a given skill ID (if specified)
    pub fn get_allowed_tools(&self, skill_id: &str) -> Option<&[String]> {
        self.parsed_cache.get(skill_id).and_then(|s| {
            if s.metadata.allowed_tools.is_empty() {
                None
            } else {
                Some(s.metadata.allowed_tools.as_slice())
            }
        })
    }
}
