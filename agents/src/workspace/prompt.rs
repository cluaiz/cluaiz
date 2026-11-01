//! A workspace in the system prompt, within a budget that follows the model's context.
//!
//! Three parts, each led by a managed prompt (`cluaiz_core::prompts`):
//!
//! 1. **Instructions** (`workspace.instructions`): the AGENTS.md files, farthest first; the nearest
//!    files take the budget first. A file that does not fit whole is cut, and says how to read the
//!    rest.
//! 2. **Rules** (`workspace.rules`): every `always_on` rule, whole. A rule is never cut: one that
//!    does not fit goes to the index instead.
//! 3. **Index** (`workspace.index`): one line per `model_decision` and `glob` rule, workflow and
//!    skill (and per always-on rule that did not fit): its name and description. The model reads
//!    one with `read_item`. `manual` rules are left out (the user names them).
//!
//! The index is placed first in the budget, because without it the model cannot find anything.

use cluaiz_core::prompts;

use super::{Kind, Trigger, Workspace};

/// The name of the meta-tool that reads an item
pub const READ_ITEM: &str = "read_item";
/// The share of the context the workspace may use, as characters per context token: at about 4
/// characters a token this is a quarter of the context
const CHARS_PER_CONTEXT_TOKEN: usize = 1;
/// Most characters for the workspace in any context
const MAX_CHARS: usize = 48_000;
/// Most characters of one index line's description
const INDEX_DESCRIPTION_CHARS: usize = 160;

/// The characters the workspace may use for a context of `n_ctx` tokens
pub fn budget_for(n_ctx: usize) -> usize {
    (n_ctx * CHARS_PER_CONTEXT_TOKEN).min(MAX_CHARS)
}

/// How one file ended up in the prompt
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// Its whole text is in the prompt
    Whole,
    /// Part of its text is in the prompt
    Cut,
    /// Its name and description are in the index
    Index,
    /// Not in the prompt (no room, or a manual rule)
    Left,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Part {
    /// "instructions", "rule", "workflow" or "skill"
    pub kind: String,
    pub name: String,
    pub placement: Placement,
    /// Characters it takes in the prompt
    pub chars: usize,
    /// Characters of its file
    pub total: usize,
}

/// The workspace's part of the system prompt and what went where
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct WorkspacePrompt {
    pub text: String,
    pub budget: usize,
    pub parts: Vec<Part>,
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Rule => "rule",
        Kind::Workflow => "workflow",
        Kind::Skill => "skill",
    }
}

impl Workspace {
    /// The workspace's part of the system prompt within `budget` characters
    pub fn prompt(&self, budget: usize) -> WorkspacePrompt {
        let mut parts: Vec<Part> = Vec::new();
        // The blank lines between the three sections
        let budget_left = budget.saturating_sub(4);

        // Always-on rules with their text; the ones that do not fit whole join the index
        let always: Vec<(&super::Item, String)> = self
            .rules
            .iter()
            .filter(|rule| rule.trigger == Some(Trigger::AlwaysOn))
            .map(|rule| (rule, rule.body().unwrap_or_default()))
            .collect();

        // 1. The index of everything read on demand: placed first. Room is also kept for the
        //    index line of every always-on rule, in case it does not fit whole.
        let index_line = |item: &super::Item, note: &str| {
            let description: String = item.description.chars().take(INDEX_DESCRIPTION_CHARS).collect();
            let globs = if item.trigger == Some(Trigger::Glob) { format!(" (for files {})", item.globs.join(", ")) } else { String::new() };
            format!("\n- {} ({}{note}){globs}: {}", item.name, kind_name(item.kind), if description.is_empty() { "-" } else { &description })
        };
        let always_lines: Vec<String> = always.iter().map(|(rule, _)| index_line(rule, ", always on")).collect();
        let on_demand: Vec<&super::Item> = self
            .index()
            .filter(|item| !matches!(item.trigger, Some(Trigger::AlwaysOn) | Some(Trigger::Manual)))
            .collect();
        let index_heading = prompts::text("workspace.index");
        let mut reserve: usize = always_lines.iter().map(String::len).sum();
        let mut used = index_heading.len();
        let mut index_lines: Vec<String> = Vec::new();
        for item in &on_demand {
            let line = index_line(item, "");
            if used + reserve + line.len() > budget_left {
                parts.push(Part { kind: kind_name(item.kind).into(), name: item.name.clone(), placement: Placement::Left, chars: 0, total: item.description.len() });
                continue;
            }
            used += line.len();
            parts.push(Part { kind: kind_name(item.kind).into(), name: item.name.clone(), placement: Placement::Index, chars: line.len(), total: item.description.len() });
            index_lines.push(line);
        }
        if index_lines.is_empty() && always.is_empty() {
            used = 0;
        }

        // 2. Instructions: the nearest files take what is left first
        let heading = prompts::text("workspace.instructions");
        let available = budget_left.saturating_sub(used + reserve);
        let mut left = available.saturating_sub(heading.len());
        let mut kept: Vec<(usize, String)> = Vec::new();
        for (i, file) in self.instructions.iter().enumerate().rev() {
            let total = file.text.chars().count();
            let header = format!("\n\n## {}\n", file.label);
            if left <= header.len() + 200 {
                parts.push(Part { kind: "instructions".into(), name: file.label.clone(), placement: Placement::Left, chars: 0, total });
                continue;
            }
            let room = left - header.len();
            let (text, placement) = if file.text.len() <= room {
                (file.text.clone(), Placement::Whole)
            } else {
                let note = format!("\n{}", prompts::render("workspace.cut", &[("name", &file.label)]));
                (format!("{}{note}", prefix(&file.text, room.saturating_sub(note.len()))), Placement::Cut)
            };
            let block = format!("{header}{text}");
            left = left.saturating_sub(block.len());
            parts.push(Part { kind: "instructions".into(), name: file.label.clone(), placement, chars: block.chars().count(), total });
            kept.push((i, block));
        }
        kept.sort_by_key(|(i, _)| *i);
        let mut instructions_text = String::new();
        if !kept.is_empty() {
            instructions_text.push_str(&heading);
            for (_, block) in &kept {
                instructions_text.push_str(block);
            }
        } else {
            left = available;
        }

        // 3. Always-on rules, whole when they fit (in what is left and in their own index line),
        //    else in the index; never cut
        let rules_heading = prompts::text("workspace.rules");
        let mut rules_text = String::new();
        let mut room = (left + reserve).saturating_sub(rules_heading.len());
        for ((rule, body), line) in always.iter().zip(&always_lines) {
            reserve -= line.len();
            let block = format!("\n\n## {}\n{}", rule.name, body);
            if !body.is_empty() && block.len() + reserve <= room {
                room -= block.len();
                rules_text.push_str(&block);
                parts.push(Part { kind: "rule".into(), name: rule.name.clone(), placement: Placement::Whole, chars: block.chars().count(), total: body.chars().count() });
            } else {
                // No room for the whole rule: the model finds it in the index and reads it
                room = room.saturating_sub(line.len());
                index_lines.push(line.clone());
                parts.push(Part { kind: "rule".into(), name: rule.name.clone(), placement: Placement::Index, chars: line.len(), total: body.chars().count() });
            }
        }
        // A context too small even for the index: the last index lines are left out
        let fixed = [&instructions_text, &rules_text].iter().map(|text| text.len()).sum::<usize>()
            + if rules_text.is_empty() { 0 } else { rules_heading.len() };
        while !index_lines.is_empty() && fixed + index_heading.len() + index_lines.iter().map(String::len).sum::<usize>() > budget_left {
            let line = index_lines.pop().unwrap_or_default();
            if let Some(part) = parts.iter_mut().rev().find(|part| part.placement == Placement::Index && part.chars == line.len()) {
                part.placement = Placement::Left;
                part.chars = 0;
            }
        }
        for rule in self.rules.iter().filter(|rule| rule.trigger == Some(Trigger::Manual)) {
            parts.push(Part { kind: "rule".into(), name: rule.name.clone(), placement: Placement::Left, chars: 0, total: 0 });
        }

        let mut sections = Vec::new();
        if !instructions_text.is_empty() {
            sections.push(instructions_text);
        }
        if !rules_text.is_empty() {
            sections.push(format!("{rules_heading}{rules_text}"));
        }
        if !index_lines.is_empty() {
            sections.push(format!("{index_heading}{}", index_lines.concat()));
        }
        WorkspacePrompt { text: sections.join("\n\n"), budget, parts }
    }

    /// What `read_item` returns for a name: an item of the index, or an instruction file by its
    /// label (`.agents/AGENTS.md`). Err lists the names it knows.
    pub fn read_item(&self, name: &str, kind: Option<&str>) -> Result<(String, String), String> {
        let wanted = name.trim().trim_start_matches('/');
        let kind = kind.and_then(|kind| match kind.trim().to_ascii_lowercase().as_str() {
            "rule" => Some(Kind::Rule),
            "workflow" => Some(Kind::Workflow),
            "skill" => Some(Kind::Skill),
            _ => None,
        });
        if let Some(item) = self.find(wanted, kind) {
            let body = item.body()?;
            let heading = match item.kind {
                Kind::Skill => format!("Skill {} (its files are in {})", item.name, item.folder().display()),
                Kind::Rule => format!("Rule {}", item.name),
                Kind::Workflow => format!("Workflow {}", item.name),
            };
            return Ok((format!("{} · {} lines", item.name, body.lines().count()), format!("{heading}:\n\n{body}")));
        }
        if let Some(file) = self.instructions.iter().find(|file| file.label.eq_ignore_ascii_case(wanted)) {
            let text = std::fs::read_to_string(&file.path).map_err(|error| format!("{}: {error}", file.path.display()))?;
            let body = super::split_front_matter(&text).1;
            return Ok((format!("{} · {} lines", file.label, body.lines().count()), format!("{}:\n\n{body}", file.label)));
        }
        let names: Vec<&str> = self.index().filter(|item| item.trigger != Some(Trigger::Manual)).map(|item| item.name.as_str()).collect();
        Err(format!("there is nothing named '{wanted}'. Names: {}", names.join(", ")))
    }

    /// The `read_item` schema, when there is anything to read
    pub fn read_item_schema(&self) -> Option<serde_json::Value> {
        if self.index().next().is_none() && self.instructions.is_empty() {
            return None;
        }
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": READ_ITEM,
                "description": prompts::text("workspace.read_item"),
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "The name as listed" },
                        "kind": { "type": "string", "enum": ["rule", "workflow", "skill"], "description": "When two items have the same name: the kind in brackets after it" }
                    },
                    "required": ["name"]
                }
            }
        }))
    }
}

/// The longest start of `text` within `bytes`, ending on a character boundary
fn prefix(text: &str, bytes: usize) -> &str {
    if text.len() <= bytes {
        return text;
    }
    let mut end = bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
