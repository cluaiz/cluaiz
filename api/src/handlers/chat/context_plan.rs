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
    /// The newest `recent_turns_min` turns in full; older messages come back through recall
    Adaptive,
    /// Only the system prompt and the current message (and what recall brings back)
    None,
}

impl HistoryMode {
    /// The request's value; an unknown one is ignored with a warning (optional fields never fail)
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            None | Some("recent") => Self::Recent,
            Some("adaptive") => Self::Adaptive,
            Some("none") => Self::None,
            Some(other) => {
                tracing::warn!("⚠️ [Context] history_mode '{}' is not available (recent, adaptive or none); using recent", other);
                Self::Recent
            }
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Recent => "recent",
            Self::Adaptive => "adaptive",
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
    /// For each rendered message, the index of the request message it shows (None for the tool
    /// steps the Engine puts back)
    pub origin: Vec<Option<usize>>,
}

/// Earlier turns as the model saw them: each user message gets back the block the Engine put in
/// front of it, followed by that turn's tool steps. `HistoryMode::Adaptive` keeps the newest
/// `recent_turns` turns before the current message, `HistoryMode::None` none of them; system
/// messages are always kept.
pub fn render_history(
    messages: &[ExternalMessage],
    ledger: Option<&ChatLedger>,
    tool_role: bool,
    mode: HistoryMode,
    recent_turns: usize,
) -> Rendered {
    let mut rendered = Rendered {
        messages: Vec::with_capacity(messages.len()),
        current_key: None,
        earlier_keys: Vec::new(),
        origin: Vec::with_capacity(messages.len()),
    };
    // Repeats of a user text, counted on borrowed text (a long chat has thousands of messages)
    fn trimmed(content: &MessageContent) -> std::borrow::Cow<'_, str> {
        match content.text_ref() {
            std::borrow::Cow::Borrowed(text) => std::borrow::Cow::Borrowed(text.trim()),
            std::borrow::Cow::Owned(text) => std::borrow::Cow::Owned(text.trim().to_string()),
        }
    }
    let mut occurrences: std::collections::HashMap<std::borrow::Cow<str>, usize> = std::collections::HashMap::new();
    let last_index = messages.len().saturating_sub(1);
    // First message shown: the start of the oldest kept turn
    let first_shown = match mode {
        HistoryMode::Recent => 0,
        HistoryMode::None => last_index,
        HistoryMode::Adaptive => messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.role.eq_ignore_ascii_case("user"))
            .map(|(i, _)| i)
            .rev()
            .nth(recent_turns)
            .unwrap_or(0),
    };
    for (index, original) in messages.iter().enumerate() {
        let is_system = original.role.eq_ignore_ascii_case("system");
        if index < first_shown && !is_system {
            // Its turn key still counts, so repeated texts keep their keys
            if original.role.eq_ignore_ascii_case("user") {
                *occurrences.entry(trimmed(&original.content)).or_insert(0) += 1;
            }
            continue;
        }
        let mut message = original.clone();
        let mut steps = Vec::new();
        if message.role.eq_ignore_ascii_case("user") {
            let text = message.content.text_content();
            let seen = occurrences.entry(trimmed(&original.content)).or_insert(0);
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
        rendered.origin.push(Some(index));
        for (role, content) in steps {
            rendered.messages.push(ExternalMessage { role: role.to_string(), content: MessageContent::Text(content), ..Default::default() });
            rendered.origin.push(None);
        }
    }
    rendered
}

/// Request messages the prompt does not show: those not rendered, and those in segments the
/// plan leaves out (the engine drops them to fit). `origin` must match `plan`'s messages.
pub fn hidden_messages(message_count: usize, origin: &[Option<usize>], plan: &Result<Plan, PlanError>) -> Vec<usize> {
    let mut shown = vec![false; message_count];
    for index in origin.iter().flatten() {
        shown[*index] = true;
    }
    if let Ok(plan) = plan {
        for dropped in &plan.dropped {
            let rendered = dropped.id.strip_prefix('m').and_then(|i| i.parse::<usize>().ok());
            if let Some(Some(index)) = rendered.and_then(|i| origin.get(i)) {
                shown[*index] = false;
            }
        }
    }
    (0..message_count.saturating_sub(1)).filter(|i| !shown[*i]).collect()
}

/// Adds what the Engine puts into this request: the tool hint in front of the current user
/// message, and the bound tools at the end of the system prompt (a system message is added when
/// there is none; `origin` follows)
pub fn inject(messages: &mut Vec<ExternalMessage>, origin: &mut Vec<Option<usize>>, hint: &str, system_tools: &str) {
    if !hint.is_empty() {
        match messages.last_mut() {
            Some(last) if last.role.eq_ignore_ascii_case("user") => prepend_text(&mut last.content, hint),
            _ => tracing::warn!("⚠️ [ChatHandler] Last message is not the user's; tool hint left out"),
        }
    }
    if system_tools.is_empty() {
        return;
    }
    match messages.first_mut() {
        Some(first) if first.role.eq_ignore_ascii_case("system") => match &mut first.content {
            MessageContent::Text(text) => {
                text.push_str("\n\n");
                text.push_str(system_tools);
            }
            MessageContent::Array(parts) => parts.push(super::types::ContentPart::Text { text: system_tools.to_string() }),
        },
        _ => {
            messages.insert(
                0,
                ExternalMessage { role: "system".to_string(), content: MessageContent::Text(system_tools.to_string()), ..Default::default() },
            );
            origin.insert(0, None);
        }
    }
}

/// Puts `block` in front of a message's text
pub fn prepend_text(content: &mut MessageContent, block: &str) {
    match content {
        MessageContent::Text(text) => *text = format!("{block}\n\n{text}"),
        MessageContent::Array(parts) => parts.insert(0, super::types::ContentPart::Text { text: block.to_string() }),
    }
}

/// The planner policy of a request: the settings' `context_runtime`, with the request's
/// `context_policy` fields on top. Unknown keys and invalid values are ignored with a warning.
pub fn policy(overrides: Option<&serde_json::Value>) -> Policy {
    let base = cluaiz_core::Settings::current().context_runtime;
    let Some(overrides) = overrides else { return base };
    let Some(fields) = overrides.as_object() else {
        tracing::warn!("⚠️ [Context] context_policy must be an object; ignored");
        return base;
    };
    let mut value = serde_json::to_value(&base).unwrap_or_default();
    for (key, field) in fields {
        if value.get(key).is_none() {
            tracing::warn!("⚠️ [Context] Unknown context_policy field '{}' ignored", key);
            continue;
        }
        let mut candidate = value.clone();
        candidate[key] = field.clone();
        match serde_json::from_value::<Policy>(candidate.clone()) {
            Ok(policy) if valid(&policy) => value = candidate,
            _ => tracing::warn!("⚠️ [Context] Invalid context_policy value for '{}' ignored", key),
        }
    }
    serde_json::from_value(value).unwrap_or(base)
}

/// Percentages are 0–100; at most 50 recent turns are protected
pub fn valid(policy: &Policy) -> bool {
    matches!(policy.indexer.as_str(), "auto" | "always" | "rules_only" | "off")
        && matches!(policy.notes.as_str(), "ask" | "auto" | "off")
        && matches!(policy.cross_chat.as_str(), "ask" | "allow" | "deny")
        && matches!(policy.tool_format.as_str(), "inline" | "native")
        && matches!(policy.auto_load.as_str(), "off" | "on")
        && matches!(policy.mcp_prewarm.as_str(), "off" | "on")
        && policy.recent_turns_min <= 50
        && [policy.evidence_cap_pct, policy.suggestions_cap_pct, policy.margin_pct, policy.compact_at_pct, policy.compact_to_pct]
            .iter()
            .all(|pct| *pct <= 100)
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
    policy: &Policy,
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
    cluaiz_context::plan(&PlanInput { n_ctx, reserve_answer, thinking, segments, policy: policy.clone() })
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
    policy: &Policy,
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
pub fn report(
    plan: &Result<Plan, PlanError>,
    mode: HistoryMode,
    policy: &Policy,
    compaction: Option<&Compaction>,
) -> serde_json::Value {
    match plan {
        Ok(plan) => json!({
            "history_mode": mode.name(),
            "policy": policy,
            "n_ctx": plan.n_ctx,
            "reserve": { "answer": plan.reserve_answer, "thinking": plan.reserve_thinking, "margin": plan.margin },
            "protected_tokens": plan.protected,
            "available_tokens": plan.available,
            "used_tokens": plan.used,
            "messages_over_budget": plan.dropped.len(),
            "needs_compaction": plan.needs_compaction,
            "compaction": compaction,
        }),
        Err(error) => json!({ "history_mode": mode.name(), "policy": policy, "error": error.to_string(), "compaction": compaction }),
    }
}

/// Reports kept per chat for `GET /v1/chat/{id}/context/report/{request_id}`
const REPORTS_PER_CHAT: usize = 20;

type Reports = std::collections::HashMap<String, std::collections::VecDeque<(String, serde_json::Value)>>;
static REPORTS: std::sync::LazyLock<std::sync::Mutex<Reports>> = std::sync::LazyLock::new(Default::default);

/// Keeps a request's context report with its chat (the newest 20 per chat, in RAM)
pub fn remember(chat: &str, request_id: &str, report: &serde_json::Value) {
    let mut reports = REPORTS.lock().unwrap_or_else(|e| e.into_inner());
    let kept = reports.entry(chat.to_string()).or_default();
    kept.retain(|(id, _)| id != request_id);
    kept.push_back((request_id.to_string(), report.clone()));
    while kept.len() > REPORTS_PER_CHAT {
        kept.pop_front();
    }
}

/// Forgets the chat's kept reports (its history was deleted)
pub fn forget_reports(chat: &str) {
    REPORTS.lock().unwrap_or_else(|e| e.into_inner()).remove(chat);
}

/// A kept report, or the ids of the chat's kept reports (newest last) when `request_id` is None
pub fn recall(chat: &str, request_id: Option<&str>) -> Option<serde_json::Value> {
    let reports = REPORTS.lock().unwrap_or_else(|e| e.into_inner());
    let kept = reports.get(chat)?;
    match request_id {
        Some(id) => kept.iter().find(|(kept_id, _)| kept_id == id).map(|(_, report)| report.clone()),
        None => Some(json!(kept.iter().map(|(id, _)| id).collect::<Vec<_>>())),
    }
}
