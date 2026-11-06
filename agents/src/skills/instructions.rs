use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use crate::tools::registry::ToolsRegistry;
use super::parser::{SkillParser, ParsedSkill};

/// The parsed `SKILL.md` of each enabled skill. Which tools a request gets is decided in
/// `tool_search.rs`.
#[derive(Default)]
pub struct SkillInstructions {
    /// Skill ID -> ParsedSkill
    parsed: HashMap<String, ParsedSkill>,
}

/// Built for the current registry generation (parsing every SKILL.md once per registry change
/// instead of on every lookup)
static SHARED: LazyLock<Mutex<Option<(u64, Arc<SkillInstructions>)>>> = LazyLock::new(|| Mutex::new(None));

impl SkillInstructions {
    /// The skills of the registry as it is now, read again only when the registry changed
    pub fn shared() -> Arc<SkillInstructions> {
        let generation = ToolsRegistry::generation();
        let mut guard = SHARED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((built_for, skills)) = guard.as_ref() {
            if *built_for == generation {
                return skills.clone();
            }
        }
        let skills = Arc::new(Self::read());
        *guard = Some((generation, skills.clone()));
        skills
    }

    fn read() -> Self {
        let mut skills = Self::default();
        let Ok(registry) = ToolsRegistry::load() else {
            return skills;
        };
        for (id, entry) in &registry.installed_tools {
            if entry.category == "skill" && entry.enabled {
                let skill_md = PathBuf::from(&entry.local_dir).join("SKILL.md");
                if skill_md.exists() {
                    if let Some(parsed) = SkillParser::parse_file(&skill_md) {
                        skills.parsed.insert(id.clone(), parsed);
                    }
                }
            }
        }
        skills
    }

    /// Returns prompt instructions for a given skill ID
    pub fn get(&self, skill_id: &str) -> Option<&str> {
        self.parsed.get(skill_id).map(|s| s.prompt_instructions.as_str())
    }
}
