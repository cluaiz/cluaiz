//! The prompt's history and its plan: how earlier turns are rendered (with what the Engine added
//! to them), when the chat's ledger is compacted, and the plan behind the context report. The
//! decisions about what fits come from the pure `cluaiz-context` planner; this module does the
//! I/O around it. Dropping the oldest turns when nothing else helps stays in the engine.

use super::types::{ExternalMessage, MessageContent};
use cluaiz_agents::ChatLedger;
use cluaiz_context::{Class, Plan, PlanError, PlanInput, Policy, Segment};
use serde_json::json;

/// Tool steps whose results are never cleared by compaction (the newest ones)
const KEEP_RECENT_STEPS: usize = 3;

/// `history_mode` of a request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryMode {
    /// The whole history; the engine drops the oldest turns when it does not fit (default)
    Recent,
    /// Only the system prompt and the current message
    None,
}

impl HistoryMode {
    /// The request's value; an unknown one is ignored with a warning (optional fields never fail)
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            None | Some("recent") => Self::Recent,
            Some("none") => Self::None,
            Some(other) => {
                tracing::warn!("⚠️ [Context] history_mode '{}' is not available (recent or none); using recent", other);
                Self::Recent
            }
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Recent => "recent",
            Self::None => "none",
        }
    }
}

/// Messages ready for the template, and the ledger keys of their user turns
pub struct Rendered {
    pub messages: Vec<ExternalMessage>,
    /// Key of the current (last) user message
    pub current_key: Option<String>,
    /// Keys of the earlier user messages, oldest first
    pub earlier_keys: Vec<String>,
}

/// Earlier turns as the model saw them: each user message gets back the block the Engine put in
/// front of it, followed by that turn's tool steps. With `HistoryMode::None` only the system
/// messages and the current message are kept.
pub fn render_history(
    messages: &[ExternalMessage],
    ledger: Option<&ChatLedger>,
    tool_role: bool,
    mode: HistoryMode,
) -> Rendered {
    let mut rendered = Rendered { messages: Vec::with_capacity(messages.len()), current_key: None, earlier_keys: Vec::new() };
    let mut occurrences: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let last_index = messages.len().saturating_sub(1);
    for (index, original) in messages.iter().enumerate() {
        let is_system = original.role.eq_ignore_ascii_case("system");
        if mode == HistoryMode::None && index != last_index && !is_system {
            continue;
        }
        let mut message = original.clone();
        let mut steps = Vec::new();
        if message.role.eq_ignore_ascii_case("user") {
            let text = message.content.text_content();
            let seen = occurrences.entry(text.trim().to_string()).or_insert(0);
            let key = cluaiz_agents::turn_key(&text, *seen);
            *seen += 1;
            if index == last_index {
                rendered.current_key = Some(key);
            } else {
                if let Some(record) = ledger.and_then(|saved| saved.record(&key)) {
                    if !record.block.is_empty() {
                        prepend_text(&mut message.content, &record.block);
                    }
                    steps = super::tool_turns::step_messages(&record.steps, tool_role);
                }
                rendered.earlier_keys.push(key);
            }
        }
        rendered.messages.push(message);
        for (role, content) in steps {
            rendered.messages.push(ExternalMessage { role: role.to_string(), content: MessageContent::Text(content) });
        }
    }
    rendered
}

/// Puts `block` in front of a message's text
pub fn prepend_text(content: &mut MessageContent, block: &str) {
    match content {
        MessageContent::Text(text) => *text = format!("{block}\n\n{text}"),
        MessageContent::Array(parts) => parts.insert(0, super::types::ContentPart::Text { text: block.to_string() }),
    }
}

/// Token counts of texts with the loaded model's tokenizer, else estimated from characters
pub fn count(host: &cluaiz_runtime::EngineHost, texts: &[&str]) -> (Vec<usize>, bool) {
    match host.count_tokens(texts) {
        Some(counts) => (counts, true),
        None => (texts.iter().map(|t| cluaiz_agents::tool_search::tokens(t.len())).collect(), false),
    }
}

/// The plan of `messages` in real tokens: system messages and the current (last) message are
/// protected; the other messages are history, grouped by turn (a user message starts one)
pub fn plan_messages(
    host: &cluaiz_runtime::EngineHost,
    messages: &[ExternalMessage],
    n_ctx: usize,
    reserve_answer: usize,
    thinking: bool,
) -> Result<Plan, PlanError> {
    let texts: Vec<String> = messages.iter().map(|m| m.content.text_content()).collect();
    let (counts, _) = count(host, &texts.iter().map(String::as_str).collect::<Vec<_>>());
    let last = messages.len().saturating_sub(1);
    let mut turn = 0;
    let segments = messages
        .iter()
        .zip(counts)
        .enumerate()
        .map(|(index, (message, tokens))| {
            if message.role.eq_ignore_ascii_case("user") {
                turn += 1;
            }
            let class = if index == last || message.role.eq_ignore_ascii_case("system") {
                Class::Protected
            } else {
                Class::History
            };
            Segment { id: format!("m{index}"), tokens, class, turn, group: None, score: 0.0 }
        })
        .collect();
    cluaiz_context::plan(&PlanInput { n_ctx, reserve_answer, thinking, segments, policy: Policy::default() })
}

/// What a compaction changed
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Compaction {
    pub freed_tokens: usize,
    pub blocks_removed: usize,
    pub results_cleared: usize,
    /// Tools whose load result was cleared: they are unloaded (and can be loaded again)
    pub unloaded_tools: Vec<String>,
}

/// Compaction of the chat's ledger (decided by `cluaiz_context::compact`): tool hints of older
/// turns go, then results of older tool steps become short placeholders. Changes nothing unless
/// it frees at least `clear_at_least` tokens. Only turns in `keys` (the ones this request
/// renders) are touched.
pub fn compact(
    ledger: &ChatLedger,
    keys: &[String],
    count_one: &dyn Fn(&str) -> usize,
    clear_at_least: usize,
    need: usize,
) -> Option<(ChatLedger, Compaction)> {
    let placeholder = |name: &str, tokens: usize| format!("[Result of {name} cleared to save context ({tokens} tokens).]");
    // Rendered turns, oldest first (the order of the request)
    let rendered: Vec<usize> = keys.iter().filter_map(|key| ledger.turns.iter().position(|turn| &turn.key == key)).collect();
    let turns = rendered
        .iter()
        .map(|&t| {
            let turn = &ledger.turns[t];
            cluaiz_context::TurnCost {
                block_tokens: if turn.block.is_empty() { 0 } else { count_one(&turn.block) },
                steps: turn
                    .steps
                    .iter()
                    .map(|step| {
                        // Placeholders and unload notes are in brackets: never cleared again
                        let clearable = !step.result.starts_with('[');
                        let tokens = if clearable { count_one(&step.result) } else { 0 };
                        cluaiz_context::StepCost {
                            tokens,
                            placeholder_tokens: if clearable { count_one(&placeholder(&step.name, tokens)) } else { 0 },
                            clearable,
                        }
                    })
                    .collect(),
            }
        })
        .collect();
    let policy = Policy::default();
    let plan = cluaiz_context::compact(&cluaiz_context::CompactionInput {
        turns,
        clear_at_least,
        need,
        recent_turns_min: policy.recent_turns_min,
        keep_recent_steps: KEEP_RECENT_STEPS,
    })?;
    let mut compacted = ledger.clone();
    for &t in &plan.blocks {
        compacted.turns[rendered[t]].block.clear();
    }
    let mut unloaded_tools = Vec::new();
    for &(t, s) in &plan.results {
        let step = &mut compacted.turns[rendered[t]].steps[s];
        // A cleared load is an unload: the note says so, and the caller unbinds the tool
        if let Some(id) = step.loaded_tool() {
            step.result = format!("[Tool '{id}' was unloaded to save context; load it again if needed.]");
            unloaded_tools.push(id);
        } else {
            let tokens = count_one(&step.result);
            step.result = placeholder(&step.name, tokens);
        }
    }
    Some((
        compacted,
        Compaction {
            freed_tokens: plan.freed_tokens,
            blocks_removed: plan.blocks.len(),
            results_cleared: plan.results.len(),
            unloaded_tools,
        },
    ))
}

/// The `plan` part of the context report
pub fn report(plan: &Result<Plan, PlanError>, mode: HistoryMode, compaction: Option<&Compaction>) -> serde_json::Value {
    match plan {
        Ok(plan) => json!({
            "history_mode": mode.name(),
            "n_ctx": plan.n_ctx,
            "reserve": { "answer": plan.reserve_answer, "thinking": plan.reserve_thinking, "margin": plan.margin },
            "protected_tokens": plan.protected,
            "available_tokens": plan.available,
            "used_tokens": plan.used,
            "messages_over_budget": plan.dropped.len(),
            "needs_compaction": plan.needs_compaction,
            "compaction": compaction,
        }),
        Err(error) => json!({ "history_mode": mode.name(), "error": error.to_string(), "compaction": compaction }),
    }
}
