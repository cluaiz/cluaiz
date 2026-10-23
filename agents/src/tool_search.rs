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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

/// The catalog built for one registry generation
static CATALOG: LazyLock<Mutex<Option<(u64, Arc<Vec<CatalogEntry>>)>>> = LazyLock::new(|| Mutex::new(None));
use serde_json::{json, Value};
use crate::tools::mcp::McpClient;
use crate::tools::prompt_compiler::ToolPromptCompiler;
use crate::tools::registry::ToolsRegistry;

/// Meta-tool: search the installed tools (cards only)
pub const FIND_TOOLS_NAME: &str = "find_tools";
/// Meta-tool: load one tool's instructions or function schemas
pub const LOAD_TOOL_NAME: &str = "load_tool";
/// Meta-tool: remove a loaded tool from the conversation
pub const UNLOAD_TOOL_NAME: &str = "unload_tool";
/// Older name of `find_tools`, still accepted
pub const TOOL_SEARCH_NAME: &str = "tool_search";

/// Most cards and titles shown with one message
const MAX_CARDS: usize = 3;
const MAX_TITLES: usize = 5;
/// Most results `find_tools` returns
const MAX_FOUND: usize = 5;
/// Characters per token, for estimates before the real tokenizer is used
pub const CHARS_PER_TOKEN: f64 = 3.5;

/// Aliases, stopwords and action words (data, not code)
struct Words {
    aliases: HashMap<String, Vec<String>>,
    stopwords: HashSet<String>,
    actions: HashSet<String>,
}

static WORDS: LazyLock<Words> = LazyLock::new(|| {
    let data: Value = serde_json::from_str(include_str!("../data/tool_words.json")).unwrap_or_default();
    let list = |key: &str| -> HashSet<String> {
        data.get(key)
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let aliases = data
        .get("aliases")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(alias, terms)| {
                    let terms = terms.as_array().map(|t| t.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
                    (alias.to_lowercase(), terms)
                })
                .collect()
        })
        .unwrap_or_default();
    Words { aliases, stopwords: list("stopwords"), actions: list("actions") }
});

fn is_word_char(c: char) -> bool {
    // Devanagari vowel signs are marks, not letters, but they belong to the word
    c.is_alphanumeric() || ('\u{0900}'..='\u{097F}').contains(&c)
}

/// Raw lowercase words: split on anything that is not part of a word, and on camelCase,
/// snake_case and kebab-case boundaries
fn raw_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for c in text.chars() {
        if is_word_char(c) {
            if c.is_uppercase() && prev_lower && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.extend(c.to_lowercase());
            prev_lower = c.is_lowercase();
        } else {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            prev_lower = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Plural `s` is dropped so "files" and "file" meet
fn stem(word: &str) -> String {
    if word.len() > 3 && word.ends_with('s') && !word.ends_with("ss") && word.is_ascii() {
        word[..word.len() - 1].to_string()
    } else {
        word.to_string()
    }
}

/// Search terms of a text: words without stopwords, with aliases mapped to their meaning
pub fn terms(text: &str) -> Vec<String> {
    let words = &*WORDS;
    let mut out = Vec::new();
    for word in raw_words(text) {
        // Numbers say nothing about which tool is meant
        if words.stopwords.contains(&word) || word.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        match words.aliases.get(&word) {
            Some(meanings) => out.extend(meanings.iter().cloned()),
            None => out.push(stem(&word)),
        }
    }
    out
}

/// Terms the message implies without saying them: a file name or path means a file,
/// a link means the web, code means code
fn entity_terms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let has_file = text.split_whitespace().any(|token| {
        let token = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '/' && c != '\\');
        token.contains('/') || token.contains('\\') || token.rsplit_once('.').is_some_and(|(stem, ext)| {
            !stem.is_empty() && (1..=5).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()) && !ext.chars().all(|c| c.is_ascii_digit())
        })
    });
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

/// Whether the message asks for something to be done (a verb like read, run or create, or a
/// file, link or code block), so tools may be useful even when none matched by name
pub fn is_actionable(text: &str) -> bool {
    let words = &*WORDS;
    !entity_terms(text).is_empty() || terms(text).iter().any(|term| words.actions.contains(term))
}

/// How much of a tool the model is shown with a message
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// `id — title` (about 10–15 tokens)
    Title,
    /// A short card: kind, what it is for, load cost (about 40–60 tokens)
    Card,
}

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
fn listed_functions(category: &str, local_dir: &Path, manifest: Option<&Value>) -> Vec<Value> {
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
    // All words of the phrase, in order, within a short window (allows one word in between)
    let window = phrase.len() + 1;
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
            if !entry.id_terms.is_empty() && phrase_in(query, &entry.id_terms) {
                scored.decisive = true;
            }
            scored
        })
        .collect()
}

pub struct ToolSearch;

impl ToolSearch {
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
        matches!(name, FIND_TOOLS_NAME | LOAD_TOOL_NAME | UNLOAD_TOOL_NAME | TOOL_SEARCH_NAME)
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
    /// match on the tool's name, or two weaker matches, earn a title; one lone word earns
    /// nothing.
    pub fn suggest(catalog: &[CatalogEntry], message: &str, previous: Option<&str>) -> Vec<Candidate> {
        let mut query = terms(message);
        query.extend(entity_terms(message));
        if let Some(previous) = previous.filter(|_| query.len() <= 3) {
            query.extend(terms(previous));
            query.extend(entity_terms(previous));
        }
        let mut candidates: Vec<Candidate> = score_catalog(catalog, &query)
            .into_iter()
            .zip(catalog)
            .filter_map(|(scored, entry)| {
                // Two separate good matches (name, function name or trigger word) earn a card;
                // a lone trigger, function or description word ("protocol", "run", "text")
                // earns nothing
                let level = if scored.decisive || scored.good >= 2 {
                    Level::Card
                } else if scored.named || scored.matched.len() >= 2 {
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

    /// The `<tools_hint>` lines for the candidates: cards first, then titles
    pub fn hint_lines(catalog: &[CatalogEntry], candidates: &[Candidate]) -> String {
        let find = |id: &str| catalog.iter().find(|e| e.id == id);
        let mut out = String::new();
        let cards: Vec<&Candidate> = candidates.iter().filter(|c| c.level == Level::Card).collect();
        if !cards.is_empty() {
            out.push_str("Tools that may help. To use one, call load_tool with its id:\n");
            for candidate in cards {
                if let Some(entry) = find(&candidate.id) {
                    out.push_str(&format!(
                        "- {} ({}): {}. Load cost ~{} tokens.\n",
                        entry.id,
                        entry.category,
                        entry.summary().trim_end_matches('.'),
                        load_cost_tokens(entry)
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
        for schema in schemas {
            let line = schema.to_string();
            if used + line.len() + 1 <= room {
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
                "Not loaded for lack of room (load_tool with \"functions\" to get them): {}\n",
                left_out.join(", ")
            ));
        }
        text.push_str(&format!("When this tool is not useful, call unload_tool with id \"{}\".", entry.id));
        LoadResult { loaded: Some(entry.id), text }
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
    let functions: usize = functions.iter().map(|f| f.to_string().len() + 40).sum();
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
        assert_eq!(ids("notes.txt padho")[0], ("filesystem".into(), Level::Card));
        assert_eq!(ids("ye code chalao")[0], ("code-interpreter".into(), Level::Card));
    }

    #[test]
    fn test_terms_split_and_map_words() {
        assert_eq!(terms("readTextFile"), ["read", "text", "file"]);
        assert_eq!(terms("list_directory"), ["list", "folder"]);
        assert_eq!(terms("फाइल पढ़ो"), ["file", "read"]);
        assert!(terms("ye kya hai").is_empty());
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
        assert_eq!(c[0].id, "code-interpreter");
    }

    #[test]
    fn test_short_sentence_cuts_at_a_word() {
        assert_eq!(short_sentence("Read files. Then more.", 50), "Read files");
        assert_eq!(short_sentence("one two three four", 9), "one two…");
    }
}
