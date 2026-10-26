//! Tool suggestions and loading (progressive disclosure).
//!
//! The Engine matches the user's message against the installed, switched-on tools and shows the
//! model only short cards for the likely ones. The model loads a tool with `load_tool` when it
//! needs it (the full `SKILL.md` or the function schemas), drops it with `unload_tool`, and can
//! search the rest with `find_tools`. Nothing is injected in full unless the model asks, and
//! nothing is ever cut in the middle.
//!
//! Matching works on whole words (never on parts of words), with a small alias list for common
//! Hinglish and Devanagari words. Only the servers of loaded MCP tools are started.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

/// The catalog built for one registry generation
static CATALOG: LazyLock<Mutex<Option<(u64, Arc<Vec<CatalogEntry>>)>>> = LazyLock::new(|| Mutex::new(None));
use serde_json::{json, Value};
use crate::tools::mcp::McpClient;
use crate::tools::prompt_compiler::ToolPromptCompiler;
use crate::tools::registry::ToolsRegistry;
pub use cluaiz_context::analyze::terms;
use cluaiz_context::analyze::raw_words;

/// Meta-tool: search the installed tools (cards only)
pub const FIND_TOOLS_NAME: &str = "find_tools";
/// Meta-tool: load one tool's instructions or function schemas
pub const LOAD_TOOL_NAME: &str = "load_tool";
/// Meta-tool: remove a loaded tool from the conversation
pub const UNLOAD_TOOL_NAME: &str = "unload_tool";
/// Older name of `find_tools`, still accepted
pub const TOOL_SEARCH_NAME: &str = "tool_search";
/// Meta-tool: read one more file of a tool (its references, examples, …), never cut
pub const READ_TOOL_FILE_NAME: &str = "read_tool_file";
/// Files listed for a tool (the rest are not named)
const MAX_LISTED_FILES: usize = 40;

/// Most cards and titles shown with one message
const MAX_CARDS: usize = 3;
const MAX_TITLES: usize = 5;
/// Most results `find_tools` returns
const MAX_FOUND: usize = 5;
/// Functions `load_tool` sends in full when no list is given
const MAX_FUNCTIONS_PER_LOAD: usize = 6;
/// Characters per token, for estimates before the real tokenizer is used
pub const CHARS_PER_TOKEN: f64 = 3.5;

/// Terms the message implies without saying them: a file name or path means a file,
/// a link means the web, code means code
fn entity_terms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let has_file = text.split_whitespace().any(is_file_token);
    if has_file && !text.contains("://") {
        out.push("file".to_string());
    }
    if text.contains("http://") || text.contains("https://") || text.contains("www.") {
        out.extend(["fetch", "url", "web"].map(str::to_string));
    }
    if text.contains("```") {
        out.push("code".to_string());
    }
    out
}

/// A file name or path ("notes.txt", "src/main.rs")
fn is_file_token(token: &str) -> bool {
    let token = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '/' && c != '\\');
    token.contains('/') || token.contains('\\') || token.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && (1..=5).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()) && !ext.chars().all(|c| c.is_ascii_digit())
    })
}

/// A link ("https://example.com", "www.example.com")
fn is_link_token(token: &str) -> bool {
    token.contains("://") || token.starts_with("www.")
}

/// Whether the message names something tools work on (a file, path, link or code block), so
/// tools may be useful even when none matched by name. Read from the message's form, not from
/// verbs of any language.
pub fn is_actionable(text: &str) -> bool {
    !entity_terms(text).is_empty()
}

/// How much of a tool the model is shown with a message
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// `id — title` (about 10–15 tokens)
    Title,
    /// A short card: kind, what it is for, load cost (about 40–60 tokens)
    Card,
}

/// How far the one card must score above every other candidate for `auto_load`
pub const AUTO_LOAD_MARGIN: f64 = 2.0;

/// A tool that matched the message
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    pub level: Level,
    pub score: f64,
    /// Matched terms, for logs and the context report
    pub matched: Vec<String>,
}

/// A switched-on skill, plugin or MCP server as the matcher sees it
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub id: String,
    pub category: String,
    pub name: String,
    pub title: String,
    pub description: String,
    id_terms: Vec<String>,
    name_terms: HashSet<String>,
    trigger_terms: HashSet<String>,
    trigger_phrases: Vec<Vec<String>>,
    function_terms: HashSet<String>,
    function_description_terms: HashSet<String>,
    description_terms: HashSet<String>,
    /// Estimated tokens `load_tool` adds (instructions plus known functions)
    load_cost: usize,
}

impl CatalogEntry {
    fn from_registry(entry: &crate::tools::registry::ToolEntry) -> Self {
        let local_dir = PathBuf::from(&entry.local_dir);
        let manifest = read_manifest(&local_dir);
        let title = manifest
            .as_ref()
            .and_then(|m| m.get("title").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_default();
        let functions = listed_functions(&entry.category, &local_dir, manifest.as_ref());
        let mut function_terms = HashSet::new();
        let mut function_description_terms = HashSet::new();
        for function in &functions {
            if let Some(name) = function.get("name").and_then(Value::as_str) {
                function_terms.extend(terms(name));
            }
            if let Some(description) = function.get("description").and_then(Value::as_str) {
                function_description_terms.extend(terms(description));
            }
        }
        let mut trigger_terms = HashSet::new();
        let mut trigger_phrases = Vec::new();
        for trigger in &entry.semantic_triggers {
            let phrase = terms(trigger);
            match phrase.len() {
                0 => {}
                1 => {
                    trigger_terms.extend(phrase);
                }
                _ => trigger_phrases.push(phrase),
            }
        }
        CatalogEntry {
            id: entry.id.clone(),
            category: entry.category.clone(),
            name: entry.name.clone(),
            title,
            description: entry.description.clone(),
            id_terms: terms(&entry.id),
            name_terms: terms(&entry.name).into_iter().chain(terms(&entry.id)).collect(),
            trigger_terms,
            trigger_phrases,
            function_terms,
            function_description_terms,
            description_terms: terms(&format!("{} {}", entry.description, manifest_title_or_empty(&manifest))).into_iter().collect(),
            load_cost: estimate_load_cost(&local_dir, &functions),
        }
    }

    /// Estimated tokens `load_tool` adds for this tool
    pub fn load_cost(&self) -> usize {
        self.load_cost
    }

    /// What the tool is for, in one short line
    pub fn summary(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        short_sentence(&self.description, 120)
    }
}

fn manifest_title_or_empty(manifest: &Option<Value>) -> String {
    manifest.as_ref().and_then(|m| m.get("title").and_then(Value::as_str)).unwrap_or("").to_string()
}

fn read_manifest(local_dir: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(local_dir.join("package.json")).ok()?).ok()
}

/// Functions a tool offers without starting it: an MCP server's last known tool list, or the
/// `functions` / `tools` of a plugin's or skill's manifest
pub fn listed_functions(category: &str, local_dir: &Path, manifest: Option<&Value>) -> Vec<Value> {
    if category == "mcp" {
        return McpClient::get_cached_tools(local_dir).unwrap_or_default();
    }
    manifest
        .and_then(|m| m.get("functions").or_else(|| m.get("tools")))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The first sentence of `text`, at most `max` characters, cut at a word
fn short_sentence(text: &str, max: usize) -> String {
    let first = text.split_terminator(['.', '\n']).next().unwrap_or(text).trim();
    if first.chars().count() <= max {
        return first.to_string();
    }
    let mut out = String::new();
    for word in first.split_whitespace() {
        if out.chars().count() + word.chars().count() + 1 > max {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    format!("{out}…")
}

/// Whether `phrase` appears in `text` as whole words (never as part of a word)
pub fn contains_phrase(text: &str, phrase: &str) -> bool {
    let phrase = terms(phrase);
    !phrase.is_empty() && phrase_in(&terms(text), &phrase)
}

fn phrase_in(query: &[String], phrase: &[String]) -> bool {
    // All words of the phrase, in order, within a short window: two words may sit in between
    // (small words like "a", "the", "ka", "ko" are kept, in every language)
    let window = phrase.len() + 2;
    (0..query.len()).any(|start| {
        let mut next = 0;
        for word in query.iter().skip(start).take(window) {
            if next < phrase.len() && *word == phrase[next] {
                next += 1;
            }
        }
        next == phrase.len() && query.get(start) == phrase.first()
    })
}

/// Query terms of one message. File names and links count as what they are ("file", "web"),
/// not by their words: "notes.txt" is a file, not a note.
fn message_terms(message: &str) -> Vec<String> {
    let plain: Vec<&str> = message.split_whitespace().filter(|t| !is_file_token(t) && !is_link_token(t)).collect();
    let mut query = terms(&plain.join(" "));
    query.extend(entity_terms(message));
    query
}

/// A message this short (in words) borrows the previous one's terms ("isko phir chalao",
/// "again"). Without words of any language a closing "thanks, that's all" looks the same, so
/// what is borrowed earns at most a title, never a card.
const FOLLOW_UP_WORDS: usize = 4;

/// Query terms of a message, and whether the previous message's terms were borrowed (a short
/// message, in any language)
fn query_terms(message: &str, previous: Option<&str>) -> (Vec<String>, bool) {
    let mut query = message_terms(message);
    let short = raw_words(message).len() <= FOLLOW_UP_WORDS;
    let borrowed = match previous.filter(|_| short) {
        Some(previous) => {
            query.extend(message_terms(previous));
            true
        }
        None => false,
    };
    (query, borrowed)
}

/// How well one tool matched the query terms
struct Scored {
    score: f64,
    matched: Vec<String>,
    /// The tool was named, or one of its trigger phrases appeared
    decisive: bool,
    /// Matches on the name, a function name or a trigger word (not just the description)
    good: usize,
    /// At least one match on the tool's own name
    named: bool,
}

/// Scores of the catalog for the query terms. A term counts once per tool, at its strongest
/// field; rarer terms (across the catalog) count more.
fn score_catalog(catalog: &[CatalogEntry], query: &[String]) -> Vec<Scored> {
    let total = catalog.len().max(1) as f64;
    let unique: Vec<&String> = {
        let mut seen = HashSet::new();
        query.iter().filter(|t| seen.insert(t.as_str())).collect()
    };
    let document_frequency = |term: &str| -> f64 {
        catalog
            .iter()
            .filter(|e| {
                e.name_terms.contains(term)
                    || e.trigger_terms.contains(term)
                    || e.function_terms.contains(term)
                    || e.function_description_terms.contains(term)
                    || e.description_terms.contains(term)
            })
            .count() as f64
    };
    catalog
        .iter()
        .map(|entry| {
            let mut scored = Scored { score: 0.0, matched: Vec::new(), decisive: false, good: 0, named: false };
            for term in &unique {
                let weight = if entry.name_terms.contains(*term) {
                    scored.named = true;
                    scored.good += 1;
                    3.0
                } else if entry.function_terms.contains(*term) {
                    scored.good += 1;
                    2.0
                } else if entry.trigger_terms.contains(*term) {
                    scored.good += 1;
                    2.0
                } else if entry.function_description_terms.contains(*term) || entry.description_terms.contains(*term) {
                    1.0
                } else {
                    continue;
                };
                scored.score += weight * (1.0 + total / document_frequency(term).max(1.0)).ln();
                scored.matched.push((*term).clone());
            }
            if entry.trigger_phrases.iter().any(|p| phrase_in(query, p)) {
                scored.score += 2.5;
                scored.decisive = true;
            }
            // Naming the tool is decisive, except a one-word id said with nothing else that
            // fits the tool ("fetch me a coffee", "that git was rude")
            if !entry.id_terms.is_empty() && phrase_in(query, &entry.id_terms) && (entry.id_terms.len() > 1 || scored.matched.len() > 1) {
                scored.decisive = true;
            }
            scored
        })
        .collect()
}

pub struct ToolSearch;

impl ToolSearch {
    /// Candidates as JSON for reports and the discover endpoint
    pub fn candidates_json(catalog: &[CatalogEntry], candidates: &[Candidate]) -> Value {
        Value::Array(
            candidates
                .iter()
                .map(|c| {
                    let entry = catalog.iter().find(|e| e.id == c.id);
                    json!({
                        "id": c.id,
                        "level": if c.level == Level::Card { "card" } else { "title" },
                        "score": (c.score * 100.0).round() / 100.0,
                        "matched": c.matched,
                        "category": entry.map(|e| e.category.clone()),
                        "summary": entry.map(|e| e.summary()),
                        "load_cost_tokens": entry.map(|e| e.load_cost()),
                    })
                })
                .collect(),
        )
    }

    /// Whether `name` is the id of an installed, switched-on tool (a component, not one of its
    /// functions)
    pub fn is_installed_tool_id(name: &str) -> bool {
        ToolsRegistry::load().is_ok_and(|registry| {
            registry.installed_tools.values().any(|entry| {
                entry.enabled
                    && entry.id.eq_ignore_ascii_case(name.trim())
                    && matches!(entry.category.as_str(), "skill" | "plugin" | "mcp")
            })
        })
    }

    /// Whether `name` is one of the Engine's tool meta-tools
    pub fn is_meta_tool(name: &str) -> bool {
        matches!(name, FIND_TOOLS_NAME | LOAD_TOOL_NAME | UNLOAD_TOOL_NAME | TOOL_SEARCH_NAME | READ_TOOL_FILE_NAME)
    }

    /// Schemas of the meta-tools (loading first: with a card shown, that is the next step)
    pub fn meta_schemas() -> Vec<Value> {
        vec![
            json!({"type": "function", "function": {
                "name": LOAD_TOOL_NAME,
                "description": "Load an installed tool by id. Returns its instructions and the functions you can then call.",
                "parameters": {"type": "object", "properties": {
                    "id": {"type": "string"},
                    "functions": {"type": "array", "items": {"type": "string"}, "description": "Optional: only these functions"}
                }, "required": ["id"]}
            }}),
            json!({"type": "function", "function": {
                "name": UNLOAD_TOOL_NAME,
                "description": "Remove a loaded tool you do not need, to free space.",
                "parameters": {"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}
            }}),
            json!({"type": "function", "function": {
                "name": READ_TOOL_FILE_NAME,
                "description": "Read one more file of a tool, such as a reference its instructions point to. An empty path lists the tool's files.",
                "parameters": {"type": "object", "properties": {"id": {"type": "string"}, "path": {"type": "string"}}, "required": ["id", "path"]}
            }}),
            json!({"type": "function", "function": {
                "name": FIND_TOOLS_NAME,
                "description": "Search the other installed tools by what they do, when none of the listed tools fits. Returns short cards.",
                "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}
            }}),
        ]
    }

    /// Switched-on skills, plugins and MCP servers, except `exclude` (already loaded). Built once
    /// per registry change and kept in RAM.
    pub fn catalog(exclude: &[String]) -> Vec<CatalogEntry> {
        let generation = ToolsRegistry::generation();
        let mut guard = CATALOG.lock().unwrap_or_else(|e| e.into_inner());
        let stale = guard.as_ref().is_none_or(|(built_for, _)| *built_for != generation);
        if stale {
            let Ok(registry) = ToolsRegistry::load() else {
                return Vec::new();
            };
            let mut entries: Vec<CatalogEntry> = registry
                .installed_tools
                .values()
                .filter(|entry| entry.enabled && matches!(entry.category.as_str(), "skill" | "plugin" | "mcp"))
                .map(CatalogEntry::from_registry)
                .collect();
            entries.sort_by(|a, b| a.id.cmp(&b.id));
            *guard = Some((generation, Arc::new(entries)));
        }
        guard
            .as_ref()
            .map(|(_, entries)| {
                entries
                    .iter()
                    .filter(|entry| !exclude.iter().any(|id| id.eq_ignore_ascii_case(&entry.id)))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Tools worth showing with this message. `previous` is the user's previous message, used
    /// when this one is a short follow-up. Strong evidence (the tool named, a trigger phrase,
    /// or two separate matches on its name, function names or trigger words) earns a card; a
    /// match on the tool's name, or a good match (function name or trigger word) with a second
    /// match or in a message that names a file, link or code, earns a title. Words found only in
    /// descriptions ("how", "are", "the" of any language) earn nothing on their own.
    pub fn suggest(catalog: &[CatalogEntry], message: &str, previous: Option<&str>) -> Vec<Candidate> {
        let (query, borrowed) = query_terms(message, previous);
        let actionable = is_actionable(message);
        let mut candidates: Vec<Candidate> = score_catalog(catalog, &query)
            .into_iter()
            .zip(catalog)
            .filter_map(|(scored, entry)| {
                // Two separate good matches (name, function name or trigger word) earn a card;
                // a lone trigger, function or description word ("protocol", "run", "text")
                // earns nothing
                let level = if (scored.decisive || scored.good >= 2) && !borrowed {
                    Level::Card
                } else if scored.named || (scored.good >= 1 && (scored.matched.len() >= 2 || actionable || borrowed)) {
                    Level::Title
                } else {
                    return None;
                };
                Some(Candidate { id: entry.id.clone(), level, score: scored.score, matched: scored.matched })
            })
            .collect();
        candidates.sort_by(|a, b| {
            (b.level == Level::Card)
                .cmp(&(a.level == Level::Card))
                .then(b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal))
                .then(a.id.cmp(&b.id))
        });
        let mut cards = 0;
        let mut titles = 0;
        candidates.retain_mut(|candidate| {
            if candidate.level == Level::Card && cards < MAX_CARDS {
                cards += 1;
                true
            } else if titles < MAX_TITLES {
                candidate.level = Level::Title;
                titles += 1;
                true
            } else {
                false
            }
        });
        candidates
    }

    /// The evidence behind the suggestions, for every tool with any match (for calibrating the
    /// levels against a labelled routing set)
    pub fn evidence(catalog: &[CatalogEntry], message: &str, previous: Option<&str>) -> Value {
        let (query, _) = query_terms(message, previous);
        Value::Array(
            score_catalog(catalog, &query)
                .into_iter()
                .zip(catalog)
                .filter(|(scored, _)| !scored.matched.is_empty() || scored.decisive)
                .map(|(scored, entry)| {
                    json!({
                        "id": entry.id,
                        "score": (scored.score * 100.0).round() / 100.0,
                        "matched": scored.matched,
                        "good": scored.good,
                        "named": scored.named,
                        "decisive": scored.decisive,
                    })
                })
                .collect(),
        )
    }

    /// The `<tools_hint>` lines for the candidates: cards first, then titles
    pub fn hint_lines(catalog: &[CatalogEntry], candidates: &[Candidate]) -> String {
        let find = |id: &str| catalog.iter().find(|e| e.id == id);
        let mut out = String::new();
        let cards: Vec<&Candidate> = candidates.iter().filter(|c| c.level == Level::Card).collect();
        if !cards.is_empty() {
            out.push_str("Tools that may help. To use one, call load_tool with its id:\n");
            for candidate in cards {
                if let Some(entry) = find(&candidate.id) {
                    // An MCP server that keeps crashing is shown, but marked
                    let unavailable = entry.category == "mcp"
                        && crate::ToolsEngine::get_tool(&entry.id)
                            .ok()
                            .flatten()
                            .is_some_and(|tool| crate::McpClient::is_unavailable(std::path::Path::new(&tool.local_dir)));
                    out.push_str(&format!(
                        "- {} ({}): {}. Load cost ~{} tokens.{}\n",
                        entry.id,
                        entry.category,
                        entry.summary().trim_end_matches('.'),
                        load_cost_tokens(entry),
                        if unavailable { " Unavailable now: its server crashed several times." } else { "" }
                    ));
                }
            }
        }
        let titles: Vec<String> = candidates
            .iter()
            .filter(|c| c.level == Level::Title)
            .filter_map(|c| find(&c.id))
            .map(|e| format!("{} — {}", e.id, short_sentence(&e.summary(), 60)))
            .collect();
        if !titles.is_empty() {
            out.push_str(&format!("Also installed: {}\n", titles.join("; ")));
        }
        out
    }

    /// `find_tools`: cards of the best matches. Unlike the suggestions, any match counts here,
    /// because the model asked.
    pub fn find(query: &str, exclude: &[String]) -> String {
        let catalog = Self::catalog(exclude);
        if catalog.is_empty() {
            return "No other tools are installed and switched on.".to_string();
        }
        let mut terms_list = terms(query);
        terms_list.extend(entity_terms(query));
        let mut scored: Vec<(f64, &CatalogEntry)> = score_catalog(&catalog, &terms_list)
            .into_iter()
            .zip(&catalog)
            .filter(|(scored, _)| scored.score > 0.0)
            .map(|(scored, entry)| (scored.score, entry))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.id.cmp(&b.1.id)));
        if scored.is_empty() {
            let ids: Vec<&str> = catalog.iter().map(|e| e.id.as_str()).collect();
            return format!(
                "No installed tool matched \"{}\". Installed tools: {}. Load one by id with load_tool if it fits.",
                query.trim(),
                ids.join(", ")
            );
        }
        let mut out = String::from("Matching tools. To use one, call load_tool with its id:\n");
        for (_, entry) in scored.into_iter().take(MAX_FOUND) {
            out.push_str(&format!(
                "- {} ({}): {}. Load cost ~{} tokens.\n",
                entry.id,
                entry.category,
                entry.summary().trim_end_matches('.'),
                load_cost_tokens(entry)
            ));
        }
        out
    }

    /// `load_tool`: the tool's instructions and function schemas, whole, within `max_chars`.
    /// A skill whose instructions do not fit is refused, never cut. For a large function list
    /// the functions closest to `query` come first and the rest are named.
    /// The `auto_load` fast path: the one tool to load before the model answers, when the
    /// suggestions name exactly one card and it scores at least `AUTO_LOAD_MARGIN` above every
    /// other candidate. On the routing set (2026-10-24) this picked 67 of 108 rows that need a
    /// tool, one of them wrong, and none of the rows that need none.
    pub fn auto_load_pick(candidates: &[Candidate]) -> Option<&Candidate> {
        let mut cards = candidates.iter().filter(|c| c.level == Level::Card);
        let card = cards.next()?;
        if cards.next().is_some() {
            return None;
        }
        let next = candidates.iter().filter(|c| c.id != card.id).map(|c| c.score).fold(f64::MIN, f64::max);
        (card.score - next >= AUTO_LOAD_MARGIN).then_some(card)
    }

    pub async fn load(id: &str, functions: &[String], query: &str, max_chars: usize) -> LoadResult {
        let Some(entry) = Self::catalog(&[]).into_iter().find(|e| e.id.eq_ignore_ascii_case(id)) else {
            let installed: Vec<String> = Self::catalog(&[]).into_iter().map(|e| e.id).collect();
            return LoadResult::refused(format!(
                "No switched-on tool has the id \"{}\". Installed tools: {}.",
                id,
                installed.join(", ")
            ));
        };
        let compiled = ToolPromptCompiler::compile_tools(std::slice::from_ref(&entry.id)).await;
        let instructions = compiled.skill_instructions;
        let room = max_chars.saturating_sub(instructions.len() + 300);
        if !instructions.is_empty() && instructions.len() + 300 > max_chars {
            return LoadResult::refused(format!(
                "Tool '{}' needs about {} tokens but only about {} are free. Unload a tool you do not need, or answer without it.",
                entry.id,
                tokens(instructions.len()),
                tokens(max_chars)
            ));
        }

        let mut schemas = compiled.function_schemas;
        if !functions.is_empty() {
            let wanted: HashSet<String> = functions.iter().map(|f| f.to_lowercase()).collect();
            schemas.retain(|s| wanted.contains(&function_name(s).to_lowercase()));
        }
        let query_terms: HashSet<String> = terms(query).into_iter().chain(entity_terms(query)).collect();
        let relevance = |schema: &Value| -> usize {
            terms(&format!("{} {}", function_name(schema), function_description(schema)))
                .iter()
                .filter(|t| query_terms.contains(*t))
                .count()
        };
        schemas.sort_by_key(|schema| std::cmp::Reverse(relevance(schema)));
        let mut kept = Vec::new();
        let mut left_out = Vec::new();
        let mut used = 0;
        // Without a list of functions, a large tool sends its most relevant functions in full
        // and names the rest (the model asks for them with "functions")
        let limit = if functions.is_empty() { MAX_FUNCTIONS_PER_LOAD } else { usize::MAX };
        for schema in schemas {
            let line = schema.to_string();
            if kept.len() < limit && used + line.len() + 1 <= room {
                used += line.len() + 1;
                kept.push(line);
            } else {
                left_out.push(function_name(&schema));
            }
        }
        if kept.is_empty() && instructions.is_empty() {
            return LoadResult::refused(if left_out.is_empty() {
                format!("Tool '{}' has nothing to load.", entry.id)
            } else {
                format!(
                    "Tool '{}' does not fit: about {} tokens are free. Unload a tool or load fewer functions (functions: {}).",
                    entry.id,
                    tokens(max_chars),
                    left_out.join(", ")
                )
            });
        }

        let mut text = format!("Loaded tool '{}' ({}).\n", entry.id, entry.category);
        if !instructions.is_empty() {
            text.push_str(&instructions);
            text.push('\n');
        }
        if !kept.is_empty() {
            text.push_str(&format!("Functions you can call now:\n<tools>\n{}\n</tools>\n", kept.join("\n")));
        }
        if !left_out.is_empty() {
            text.push_str(&format!(
                "More functions (call load_tool with \"functions\" to get them): {}\n",
                left_out.join(", ")
            ));
        }
        let files = tool_files(Path::new(&entry_dir(&entry.id)));
        if !files.is_empty() {
            text.push_str(&format!("Its other files (read one with read_tool_file): {}\n", files.join(", ")));
        }
        text.push_str(&format!("When this tool is not useful, call unload_tool with id \"{}\".", entry.id));
        LoadResult { loaded: Some(entry.id), text }
    }
}

/// `read_tool_file`: a file of a switched-on tool, whole, or the tool's files when `path` is empty
/// or not one of them. Only inside the tool's folder, and never cut: a file larger than
/// `max_chars` is refused with its size.
pub fn read_tool_file(id: &str, path: &str, max_chars: usize) -> String {
    let Some(entry) = ToolSearch::catalog(&[]).into_iter().find(|e| e.id.eq_ignore_ascii_case(id.trim())) else {
        return format!("No switched-on tool has the id \"{}\".", id.trim());
    };
    read_in_folder(&entry.id, Path::new(&entry_dir(&entry.id)), path, max_chars)
}

fn entry_dir(id: &str) -> String {
    crate::ToolsEngine::get_tool(id).ok().flatten().map(|t| t.local_dir).unwrap_or_default()
}

/// The text files of a tool folder that its load does not send already (relative paths, `/`)
fn tool_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if name.starts_with('.') || name == "scratch" || name == "node_modules" {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if !matches!(name.as_str(), "SKILL.md" | "package.json") && is_text_file(&path) {
                if let Ok(relative) = path.strip_prefix(dir) {
                    out.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    out.sort();
    out.truncate(MAX_LISTED_FILES);
    out
}

fn is_text_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref(),
        Some("md" | "txt" | "json" | "yaml" | "yml" | "toml" | "py" | "js" | "ts" | "sh" | "ps1" | "rs" | "html" | "css" | "csv" | "xml")
    )
}

fn read_in_folder(id: &str, dir: &Path, path: &str, max_chars: usize) -> String {
    let listing = || {
        let files = tool_files(dir);
        if files.is_empty() {
            format!("Tool '{id}' has no other files.")
        } else {
            format!("Files of tool '{id}': {}", files.join(", "))
        }
    };
    let path = path.trim().trim_start_matches(['/', '\\']);
    if path.is_empty() || path == "." {
        return listing();
    }
    let (Ok(root), Ok(target)) = (dir.canonicalize(), dir.join(path).canonicalize()) else {
        return format!("Tool '{id}' has no file '{path}'. {}", listing());
    };
    if !target.starts_with(&root) {
        return format!("'{path}' is outside the folder of tool '{id}'. {}", listing());
    }
    if !target.is_file() || !is_text_file(&target) {
        return format!("'{path}' is not a text file of tool '{id}'. {}", listing());
    }
    match std::fs::read_to_string(&target) {
        Ok(text) if text.len() > max_chars => format!(
            "'{path}' needs about {} tokens but only about {} are free. Unload a tool you do not need, or answer without it.",
            tokens(text.len()),
            tokens(max_chars)
        ),
        Ok(text) => format!("File '{path}' of tool '{id}':\n{text}"),
        Err(_) => format!("'{path}' of tool '{id}' could not be read as text."),
    }
}

/// What `load_tool` produced
#[derive(Debug, Clone)]
pub struct LoadResult {
    /// The tool id, when something was loaded
    pub loaded: Option<String>,
    /// The tool result text for the model
    pub text: String,
}

impl LoadResult {
    fn refused(text: String) -> Self {
        Self { loaded: None, text }
    }
}

/// Estimated tokens for `chars` characters
pub fn tokens(chars: usize) -> usize {
    (chars as f64 / CHARS_PER_TOKEN).ceil() as usize
}

/// Estimated tokens `load_tool` adds for a tool (instructions plus known functions), rounded
/// so the number reads as an estimate
fn estimate_load_cost(local_dir: &Path, functions: &[Value]) -> usize {
    let instructions = std::fs::read_to_string(local_dir.join("SKILL.md"))
        .ok()
        .and_then(|content| crate::tools::skills::SkillParser::parse_content(&content))
        .map_or(0, |parsed| parsed.prompt_instructions.len());
    // A load sends at most MAX_FUNCTIONS_PER_LOAD functions in full and names the rest
    let sizes: Vec<usize> = functions.iter().map(|f| f.to_string().len() + 40).collect();
    let full = sizes.len().min(MAX_FUNCTIONS_PER_LOAD);
    let functions = if sizes.is_empty() {
        0
    } else {
        sizes.iter().sum::<usize>() * full / sizes.len() + (sizes.len() - full) * 20
    };
    let total = tokens(instructions + functions);
    if total >= 100 { total / 50 * 50 } else { total.max(10) }
}

fn load_cost_tokens(entry: &CatalogEntry) -> usize {
    entry.load_cost
}

fn function_name(schema: &Value) -> String {
    schema.pointer("/function/name").and_then(Value::as_str).unwrap_or("?").to_string()
}

fn function_description(schema: &Value) -> String {
    schema.pointer("/function/description").and_then(Value::as_str).unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_in_folder_stays_inside_and_never_cuts() {
        let root = std::env::temp_dir().join(format!("cluaiz-rtf-{}", std::process::id()));
        let dir = root.join("code-interpreter");
        std::fs::create_dir_all(dir.join("references")).unwrap();
        std::fs::write(dir.join("SKILL.md"), "skill").unwrap();
        std::fs::write(dir.join("references").join("LANGUAGES.md"), "python: python file.py").unwrap();
        std::fs::write(root.join("secret.txt"), "outside").unwrap();
        let read = |path: &str, max: usize| read_in_folder("code-interpreter", &dir, path, max);
        assert!(read("references/LANGUAGES.md", 1000).contains("python file.py"));
        assert!(read("", 1000).contains("references/LANGUAGES.md") && !read("", 1000).contains("SKILL.md"));
        assert!(read("../secret.txt", 1000).contains("outside the folder") && !read("../secret.txt", 1000).contains("outside\n"));
        assert!(read("references/LANGUAGES.md", 5).contains("needs about"));
        assert!(read("missing.md", 1000).contains("has no file"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_auto_load_needs_one_clear_card() {
        let c = |id: &str, level: Level, score: f64| Candidate { id: id.into(), level, score, matched: Vec::new() };
        let clear = [c("filesystem", Level::Card, 7.2), c("pynotes", Level::Title, 3.6)];
        assert_eq!(ToolSearch::auto_load_pick(&clear).map(|p| p.id.as_str()), Some("filesystem"));
        // Close behind: the model chooses
        let close = [c("filesystem", Level::Card, 4.0), c("everything", Level::Title, 3.0)];
        assert!(ToolSearch::auto_load_pick(&close).is_none());
        // Two cards, or only titles: the model chooses
        let two = [c("filesystem", Level::Card, 9.0), c("git", Level::Card, 3.0)];
        assert!(ToolSearch::auto_load_pick(&two).is_none());
        assert!(ToolSearch::auto_load_pick(&[c("fetch", Level::Title, 9.0)]).is_none());
        assert!(ToolSearch::auto_load_pick(&[c("fetch", Level::Card, 3.0)]).is_some());
        assert!(ToolSearch::auto_load_pick(&[]).is_none());
    }

    fn entry(id: &str, name: &str, description: &str, triggers: &[&str], functions: &[&str]) -> CatalogEntry {
        let mut trigger_terms = HashSet::new();
        let mut trigger_phrases = Vec::new();
        for t in triggers {
            let p = terms(t);
            if p.len() == 1 { trigger_terms.extend(p) } else if p.len() > 1 { trigger_phrases.push(p) }
        }
        CatalogEntry {
            id: id.into(),
            category: "skill".into(),
            name: name.into(),
            title: String::new(),
            description: description.into(),
            id_terms: terms(id),
            name_terms: terms(name).into_iter().chain(terms(id)).collect(),
            trigger_terms,
            trigger_phrases,
            function_terms: functions.iter().flat_map(|f| terms(f)).collect(),
            function_description_terms: HashSet::new(),
            description_terms: terms(description).into_iter().collect(),
            load_cost: 0,
        }
    }

    fn catalog() -> Vec<CatalogEntry> {
        vec![
            entry("code-interpreter", "code-interpreter", "Run Python, JavaScript and more", &["python", "code", "run", "rust", "eval", "run this code"], &["execute_code"]),
            entry("tool-creator", "tool-creator", "Scaffold skills and plugins", &["create skill", "protocol", "new tool"], &["create_tool"]),
            entry("filesystem", "Filesystem Access Bridge", "Secure file system bridge", &[], &["read_text_file", "write_file", "list_directory"]),
        ]
    }

    fn ids(message: &str) -> Vec<(String, Level)> {
        ToolSearch::suggest(&catalog(), message, None).into_iter().map(|c| (c.id, c.level)).collect()
    }

    #[test]
    fn test_parts_of_words_never_match() {
        assert!(ids("I trust you").is_empty());
        assert!(ids("decode this text for me").is_empty());
        assert!(ids("what is a protocol").is_empty());
        assert!(ids("Hi").is_empty());
    }

    #[test]
    fn test_clear_requests_get_cards() {
        assert_eq!(ids("run this python: print(2+2)")[0], ("code-interpreter".into(), Level::Card));
        assert_eq!(ids("create a new skill")[0], ("tool-creator".into(), Level::Card));
        // A file named earns a title in any language; the words around it are not translated
        assert_eq!(ids("notes.txt padho")[0], ("filesystem".into(), Level::Title));
        assert_eq!(ids("notes.txt படி")[0], ("filesystem".into(), Level::Title));
    }

    #[test]
    fn test_terms_split_and_map_words() {
        assert_eq!(terms("readTextFile"), ["read", "text", "file"]);
        assert_eq!(terms("list_directory"), ["list", "directory"]);
        // Words stay as written, vowel signs inside the word, in any script
        assert_eq!(terms("फाइल पढ़ो"), ["फाइल", "पढ़ो"]);
        assert_eq!(terms("நேற்று என்ன பேசினோம்?"), ["நேற்று", "என்ன", "பேசினோம்"]);
    }

    #[test]
    fn test_actionable_messages() {
        assert!(is_actionable("mera project ka notes.txt dekh"));
        assert!(is_actionable("calc.py chalao"));
        assert!(!is_actionable("Hi, how are you?"));
    }

    #[test]
    fn test_follow_up_uses_the_previous_message() {
        let c = ToolSearch::suggest(&catalog(), "phir se", Some("run this python code"));
        assert_eq!((c[0].id.as_str(), c[0].level), ("code-interpreter", Level::Title));
    }

    #[test]
    fn test_a_longer_message_does_not_borrow_the_previous_one() {
        assert!(ToolSearch::suggest(&catalog(), "2+2 kitna hota hai?", Some("run this python code")).is_empty());
        assert_eq!(ToolSearch::suggest(&catalog(), "isko phir chalao", Some("run this python code"))[0].id, "code-interpreter");
    }

    #[test]
    fn test_closing_words_get_no_card_from_the_previous_message() {
        // A closing message looks like a follow-up: at most a title, never a card
        assert!(ToolSearch::suggest(&catalog(), "thanks, that's all", Some("read notes.txt")).iter().all(|c| c.level == Level::Title));
        assert_eq!(ToolSearch::suggest(&catalog(), "again please", Some("read notes.txt"))[0].id, "filesystem");
    }

    #[test]
    fn test_file_names_count_as_files_not_by_their_words() {
        assert!(message_terms("read notes.txt").iter().all(|t| t != "note"));
        assert_eq!(terms("vulnerabilities"), ["vulnerability"]);
    }

    #[test]
    fn test_a_one_word_id_alone_is_only_a_title() {
        let catalog = vec![entry("fetch", "fetch", "Web content fetching", &[], &["fetch_url"])];
        let c = ToolSearch::suggest(&catalog, "that fetch was rude", None);
        assert!(c.iter().all(|c| c.level == Level::Title));
    }

    #[test]
    fn test_short_sentence_cuts_at_a_word() {
        assert_eq!(short_sentence("Read files. Then more.", 50), "Read files");
        assert_eq!(short_sentence("one two three four", 9), "one two…");
    }
}
