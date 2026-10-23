//! Tool search: installed tools the prompt has no room for (or that the chat did not bind)
//! are listed by name only, and the model loads the ones it needs by calling `tool_search`
//! (Anthropic's tool search tool / Claude Code's deferred MCP tools work the same way).
//!
//! The search reads the registry, the manifests and the MCP tool lists already fetched;
//! only the servers of the tools it returns are started, to list their schemas.

use std::path::PathBuf;
use serde_json::{json, Value};
use crate::tools::mcp::McpClient;
use crate::tools::prompt_compiler::ToolPromptCompiler;
use crate::tools::registry::ToolsRegistry;

/// Function name the model calls to load tools
pub const TOOL_SEARCH_NAME: &str = "tool_search";

/// Most components one search loads
const MAX_COMPONENTS: usize = 3;

/// An installed, switched-on tool the prompt does not carry
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub id: String,
    pub category: String,
    pub description: String,
    /// Words that point at this tool: name, triggers and the names of its functions
    keywords: String,
}

/// What one `tool_search` call found
#[derive(Debug, Clone, Default)]
pub struct ToolSearchResult {
    /// Components whose tools were loaded
    pub loaded_ids: Vec<String>,
    /// Text returned to the model as the tool result
    pub text: String,
}

pub struct ToolSearch;

impl ToolSearch {
    /// The `tool_search` function schema
    pub fn schema() -> Value {
        json!({
            "type": "function",
            "function": {
                "name": TOOL_SEARCH_NAME,
                "description": "Load more installed tools. Describe what you need (for example \"read a file\" or \"search the web\") or name one with \"select:<tool id>\". Returns the tools that match; call them afterwards like any other tool.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "What the tool should do, or select:<tool id>" }
                    },
                    "required": ["query"]
                }
            }
        })
    }

    /// Switched-on skills, plugins and MCP servers, except `exclude` (already in the prompt)
    pub fn catalog(exclude: &[String]) -> Vec<CatalogEntry> {
        let Ok(registry) = ToolsRegistry::load() else {
            return Vec::new();
        };
        let mut entries: Vec<CatalogEntry> = registry
            .installed_tools
            .values()
            .filter(|entry| entry.enabled && !exclude.iter().any(|id| id.eq_ignore_ascii_case(&entry.id)))
            .filter(|entry| matches!(entry.category.as_str(), "skill" | "plugin" | "mcp"))
            .map(|entry| {
                let mut keywords = format!("{} {} {}", entry.id, entry.name, entry.semantic_triggers.join(" "));
                for name in function_names(&entry.category, &PathBuf::from(&entry.local_dir)) {
                    keywords.push(' ');
                    keywords.push_str(&name);
                }
                CatalogEntry {
                    id: entry.id.clone(),
                    category: entry.category.clone(),
                    description: entry.description.clone(),
                    keywords: keywords.replace(['_', '-'], " ").to_lowercase(),
                }
            })
            .collect();
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        entries
    }

    /// One line per catalog entry, as many as fit in `max_chars`
    pub fn catalog_lines(catalog: &[CatalogEntry], max_chars: usize) -> String {
        let mut lines = String::new();
        for entry in catalog {
            let description: String = entry.description.chars().take(100).collect();
            let line = format!("- {} ({}): {}\n", entry.id, entry.category, description.trim());
            if lines.len() + line.len() > max_chars {
                break;
            }
            lines.push_str(&line);
        }
        lines
    }

    /// Finds the tools for `query` among the catalog and returns their schemas (skills: their
    /// instructions), at most `max_chars` of them; the rest are named so the model can ask again.
    pub async fn search(query: &str, exclude: &[String], max_chars: usize) -> ToolSearchResult {
        let catalog = Self::catalog(exclude);
        let matched = rank(&catalog, query);
        if matched.is_empty() {
            let ids: Vec<&str> = catalog.iter().map(|entry| entry.id.as_str()).collect();
            let text = if ids.is_empty() {
                "No other tools are installed and switched on.".to_string()
            } else {
                format!("No tool matched \"{}\". Installed tools: {}. Try other words or select:<tool id>.", query.trim(), ids.join(", "))
            };
            return ToolSearchResult { loaded_ids: Vec::new(), text };
        }

        let words = query_words(query);
        let mut schemas: Vec<Value> = Vec::new();
        let mut instructions = Vec::new();
        let mut loaded_ids = Vec::new();
        for entry in &matched {
            let compiled = ToolPromptCompiler::compile_tools(std::slice::from_ref(&entry.id)).await;
            if compiled.function_schemas.is_empty() && compiled.skill_instructions.is_empty() {
                continue;
            }
            schemas.extend(compiled.function_schemas);
            if !compiled.skill_instructions.is_empty() {
                instructions.push(format!("Skill '{}' instructions:\n{}", entry.id, compiled.skill_instructions));
            }
            loaded_ids.push(entry.id.clone());
        }
        // Functions closest to the query first, then whole schemas while they fit
        schemas.sort_by_key(|schema| std::cmp::Reverse(score_text(&schema_text(schema), &words)));
        let mut text = String::new();
        let instructions = instructions.join("\n\n");
        let mut room = max_chars.saturating_sub(instructions.len());
        let mut kept = Vec::new();
        let mut left_out = Vec::new();
        for schema in schemas {
            let line = schema.to_string();
            if line.len() + 1 <= room {
                room -= line.len() + 1;
                kept.push(line);
            } else {
                left_out.push(function_name(&schema));
            }
        }
        if !kept.is_empty() {
            text.push_str(&format!(
                "Loaded {} tool(s) from {}. Call them like the tools above:\n<tools>\n{}\n</tools>\n",
                kept.len(),
                loaded_ids.join(", "),
                kept.join("\n")
            ));
        }
        if !left_out.is_empty() {
            text.push_str(&format!("Not loaded for lack of room (ask for one by name): {}\n", left_out.join(", ")));
        }
        if !instructions.is_empty() {
            text.push_str(&instructions);
        }
        if text.is_empty() {
            text = format!("The matching tools ({}) could not be loaded.", loaded_ids.join(", "));
            loaded_ids.clear();
        }
        ToolSearchResult { loaded_ids, text }
    }
}

/// Function names a component offers without starting it: the MCP tool list already
/// fetched (or its manifest), and a plugin's or skill's manifest
fn function_names(category: &str, local_dir: &std::path::Path) -> Vec<String> {
    let listed: Vec<Value> = if category == "mcp" {
        McpClient::get_cached_tools(local_dir).unwrap_or_default()
    } else {
        std::fs::read_to_string(local_dir.join("package.json"))
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .and_then(|manifest| manifest.get("functions").or_else(|| manifest.get("tools")).cloned())
            .and_then(|list| list.as_array().cloned())
            .unwrap_or_default()
    };
    listed
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
        .collect()
}

fn query_words(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() >= 2)
        .map(str::to_string)
        .collect()
}

fn score_text(text: &str, words: &[String]) -> usize {
    words.iter().filter(|word| text.contains(word.as_str())).count()
}

fn function_name(schema: &Value) -> String {
    schema.pointer("/function/name").and_then(Value::as_str).unwrap_or("?").to_string()
}

fn schema_text(schema: &Value) -> String {
    format!(
        "{} {}",
        function_name(schema).replace('_', " "),
        schema.pointer("/function/description").and_then(Value::as_str).unwrap_or("")
    )
    .to_lowercase()
}

/// Catalog entries for `query`, best first: `select:a,b` names them; otherwise words found in
/// the id, name, triggers and function names count twice as much as words in the description.
fn rank(catalog: &[CatalogEntry], query: &str) -> Vec<CatalogEntry> {
    let query = query.trim();
    if let Some(ids) = query.strip_prefix("select:") {
        return ids
            .split(',')
            .filter_map(|id| catalog.iter().find(|entry| entry.id.eq_ignore_ascii_case(id.trim())))
            .take(MAX_COMPONENTS)
            .cloned()
            .collect();
    }
    let words = query_words(query);
    let mut scored: Vec<(usize, &CatalogEntry)> = catalog
        .iter()
        .map(|entry| {
            let score = 2 * score_text(&entry.keywords, &words)
                + score_text(&entry.description.to_lowercase(), &words);
            (score, entry)
        })
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().take(MAX_COMPONENTS).map(|(_, entry)| entry.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, description: &str, keywords: &str) -> CatalogEntry {
        CatalogEntry {
            id: id.into(),
            category: "mcp".into(),
            description: description.into(),
            keywords: keywords.into(),
        }
    }

    #[test]
    fn test_rank_prefers_names_and_functions_over_descriptions() {
        let catalog = vec![
            entry("fetch", "Download a web page", "fetch fetch"),
            entry("filesystem", "Work with files on disk", "filesystem filesystem read text file write file"),
            entry("notes", "Keep notes that mention a file", "notes notes"),
        ];
        let ranked = rank(&catalog, "read a file");
        assert_eq!(ranked[0].id, "filesystem");
        assert!(ranked.iter().all(|entry| entry.id != "fetch"));
    }

    #[test]
    fn test_select_loads_by_id() {
        let catalog = vec![entry("fetch", "", "fetch"), entry("git", "", "git")];
        let ranked = rank(&catalog, "select:git, fetch");
        assert_eq!(ranked.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["git", "fetch"]);
        assert!(rank(&catalog, "select:missing").is_empty());
    }

    #[test]
    fn test_catalog_lines_stop_at_the_limit() {
        let catalog = vec![entry("a", "first", "a"), entry("b", "second", "b")];
        let lines = ToolSearch::catalog_lines(&catalog, 25);
        assert_eq!(lines, "- a (mcp): first\n");
    }
}
