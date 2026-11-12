//! What of a project fits the work at hand, found by the Engine instead of the model: small
//! models seldom call `read_item` on their own.
//!
//! - **A message:** the rules, workflows, skills and memory facts of the index are matched against
//!   the user's message with the installed tools' matcher (`tool_search`: whole words, rarer
//!   words count more). Only strong evidence counts: the item named, or two separate matches on
//!   its name and kind ("the cerd rule"). Their text goes into the user's turn (Z2) in a
//!   `<context>` block, whole or not at all; the system prompt does not change.
//! - **A file:** a `glob` rule applies when a file matching its `globs` is read or changed.

use std::collections::HashSet;

use cluaiz_core::prompts;

use super::{Item, Kind, Trigger, Workspace};
use crate::tool_search::{CatalogEntry, Level, ToolSearch};

/// Most items whose text is added to one message
const MAX_MATCHED: usize = 3;

/// The key of an item in the sets of what the model already has (`kind:name`)
pub fn key(item: &Item) -> String {
    format!("{}:{}", item.kind.name(), item.name.to_ascii_lowercase())
}

/// `{a,b}` written out (one level), as the published rule files use it (`*.{ts,tsx}`)
fn expand_braces(pattern: &str) -> Vec<String> {
    if let (Some(open), Some(close)) = (pattern.find('{'), pattern.find('}')) {
        if open < close {
            let (head, rest) = pattern.split_at(open);
            let (inside, tail) = rest[1..].split_at(close - open - 1);
            return inside.split(',').flat_map(|choice| expand_braces(&format!("{head}{}{}", choice.trim(), &tail[1..]))).collect();
        }
    }
    vec![pattern.to_owned()]
}

/// Whether a project file (its path from the project root, `/` separated) matches a glob. A
/// pattern without `/` matches the file name in any folder, as in `.gitignore` (`*.ts`).
pub fn glob_matches(pattern: &str, relative: &str) -> bool {
    let relative = relative.replace('\\', "/");
    let relative = relative.trim_start_matches("./");
    let name = relative.rsplit('/').next().unwrap_or(relative);
    let options = glob::MatchOptions { case_sensitive: !cfg!(windows), require_literal_separator: true, require_literal_leading_dot: false };
    expand_braces(pattern.trim()).iter().any(|pattern| {
        let pattern = pattern.trim_start_matches("./");
        let Ok(compiled) = glob::Pattern::new(pattern) else { return false };
        if pattern.contains('/') {
            compiled.matches_with(relative, options)
        } else {
            compiled.matches_with(name, options)
        }
    })
}

impl Workspace {
    /// The `glob` rules for a file of the project (its path from the root)
    pub fn rules_for_file(&self, relative: &str) -> Vec<&Item> {
        self.rules
            .iter()
            .filter(|rule| rule.trigger == Some(Trigger::Glob) && rule.globs.iter().any(|glob| glob_matches(glob, relative)))
            .collect()
    }

    /// The names of the skills that fit a message, including those the model already has
    pub fn matched_skills(&self, message: &str, previous: Option<&str>) -> Vec<String> {
        self.matched(message, previous, &HashSet::new())
            .into_iter()
            .filter(|item| item.kind == super::Kind::Skill)
            .map(|item| item.name.clone())
            .collect()
    }

    /// The items of the index that fit a message, best first, leaving out those in `known` (their
    /// text is in the system prompt or was already sent). `previous` is the user's message before
    /// this one, for a short follow-up.
    pub fn matched(&self, message: &str, previous: Option<&str>, known: &HashSet<String>) -> Vec<&Item> {
        let items: Vec<&Item> = self
            .index()
            .filter(|item| item.trigger != Some(Trigger::Manual) || mentions(message, &item.name))
            .filter(|item| !known.contains(&key(item)))
            .collect();
        if items.is_empty() {
            return Vec::new();
        }
        let catalog: Vec<CatalogEntry> =
            items.iter().map(|item| {
                let mut triggers: Vec<&str> = item.triggers.iter().map(String::as_str).collect();
                triggers.push(item.kind.name());
                CatalogEntry::named(&item.name, item.kind.name(), &item.description, &triggers)
            }).collect();
        let mut found: Vec<&Item> = Vec::new();
        for candidate in ToolSearch::suggest(&catalog, message, previous) {
            if candidate.level != Level::Card {
                continue;
            }
            for item in items.iter().filter(|item| item.name == candidate.id) {
                if found.len() < MAX_MATCHED && !found.iter().any(|other| key(other) == key(item)) {
                    found.push(item);
                }
            }
        }
        found
    }

    /// The `<context>` block for matched items within `budget` characters: each item's text
    /// whole, or (no room) its name and how to read it. The keys of the items sent whole.
    pub fn context_block(&self, items: &[&Item], budget: usize) -> Option<(String, Vec<String>)> {
        if items.is_empty() {
            return None;
        }
        let heading = prompts::text("workspace.matched");
        let mut left = budget.saturating_sub(heading.len() + "<context>\n\n</context>".len());
        let mut text = String::new();
        let mut sent = Vec::new();
        for item in items {
            let body = item.body().unwrap_or_default();
            // A skill's text links its own files (`references/...`): where they are, as read_item says
            let kind = match item.kind {
                Kind::Skill => format!("{}; its files are in {}", item.kind.name(), item.folder().display()),
                _ => item.kind.name().to_owned(),
            };
            let block = format!("\n\n## {} ({kind})\n{}", item.name, body);
            if !body.is_empty() && block.len() <= left {
                left -= block.len();
                text.push_str(&block);
                sent.push(key(item));
            } else {
                let line = format!("\n\n## {} ({kind})\n{}", item.name, prompts::render("workspace.matched_read", &[("name", &item.name)]));
                if line.len() <= left {
                    left -= line.len();
                    text.push_str(&line);
                }
            }
        }
        if text.is_empty() {
            return None;
        }
        Some((format!("<context>\n{heading}{text}\n</context>"), sent))
    }
}

/// Whether a message names an item (a manual rule is used only when named)
fn mentions(message: &str, name: &str) -> bool {
    crate::tool_search::contains_phrase(message, name)
}
