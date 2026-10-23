//! Tool calls of one answer, added to the conversation the way chat templates expect them.
//!
//! Each step adds one assistant message (the `<tool_call>`) and one tool message (its result),
//! and the whole conversation is rendered again through the model's template. The engine keeps
//! the KV cache of the unchanged beginning, so only the new step is processed, and every result
//! is in the context once (llama-server and the OpenAI API work the same way).

use super::preflight::PreparedChatContext;
use cluaiz_agents::RecordedStep;
use serde_json::{json, Value};

pub struct ToolTurns {
    steps: Vec<RecordedStep>,
    /// The template renders `tool` messages itself; otherwise results go in as user
    /// messages holding a `<tool_response>` block (what such templates would render).
    tool_role: bool,
}

impl ToolTurns {
    pub fn new(chat_template: Option<&str>) -> Self {
        Self {
            steps: Vec::new(),
            tool_role: chat_template.is_some_and(template_renders_tool_role),
        }
    }

    pub fn push(&mut self, raw_call_json: &str, name: &str, result: &str) {
        self.steps.push(RecordedStep {
            call_xml: format!("<tool_call>\n{}\n</tool_call>", raw_call_json),
            name: name.to_string(),
            result: result.to_string(),
        });
    }

    /// Replaces the results of earlier `load_tool` calls for `id` in this answer with a short
    /// note, so the next step no longer carries the unloaded tool
    pub fn tombstone(&mut self, id: &str) {
        for step in &mut self.steps {
            let call: Value = serde_json::from_str(
                step.call_xml.trim_start_matches("<tool_call>").trim_end_matches("</tool_call>").trim(),
            )
            .unwrap_or_default();
            let loads_it = call.get("name").and_then(Value::as_str) == Some(cluaiz_agents::LOAD_TOOL_NAME)
                && call
                    .pointer("/arguments/id")
                    .and_then(Value::as_str)
                    .is_some_and(|loaded| loaded.trim().eq_ignore_ascii_case(id.trim()));
            if loads_it {
                step.result = format!("[Tool '{}' was unloaded.]", id.trim());
            }
        }
    }

    /// Tools loaded with `load_tool` in this answer (and not unloaded since)
    pub fn loaded_ids(&self) -> Vec<String> {
        self.steps
            .iter()
            .filter(|step| step.result.starts_with("Loaded tool"))
            .filter_map(|step| {
                let call: Value = serde_json::from_str(
                    step.call_xml.trim_start_matches("<tool_call>").trim_end_matches("</tool_call>").trim(),
                )
                .ok()?;
                call.pointer("/arguments/id").and_then(Value::as_str).map(str::to_string)
                    .or_else(|| call.get("name").and_then(Value::as_str).map(str::to_string))
            })
            .collect()
    }

    /// The steps of this answer, for the chat's ledger
    pub fn steps(&self) -> Vec<RecordedStep> {
        self.steps.clone()
    }

    /// Characters the steps of this answer added
    pub fn chars(&self) -> usize {
        self.steps.iter().map(|step| step.call_xml.len() + step.result.len()).sum()
    }

    /// The prompt that continues the answer after the tool calls so far.
    pub fn continuation(&self, ctx: &PreparedChatContext) -> String {
        if ctx.prepared_media.is_some() {
            return self.media_pivot(ctx);
        }
        let mut envelope: Value = serde_json::from_str(&ctx.json_prompt).unwrap_or_else(|_| json!({}));
        if let Some(messages) = envelope.get_mut("messages").and_then(Value::as_array_mut) {
            messages.extend(self.messages());
        }
        envelope.to_string()
    }

    fn messages(&self) -> Vec<Value> {
        step_messages(&self.steps, self.tool_role)
            .into_iter()
            .map(|(role, content)| json!({ "role": role, "content": content }))
            .collect()
    }

    /// Media prompts are rendered again in full for every step (their embeddings cannot be
    /// reused), so the steps are written once into the last user message.
    fn media_pivot(&self, ctx: &PreparedChatContext) -> String {
        let mut history = String::new();
        for step in &self.steps {
            history.push_str(&step.call_xml);
            history.push('\n');
            history.push_str(&cluaiz_agents::ToolsEngine::format_tool_response_raw(&step.name, &step.result));
            history.push_str("\n\n");
        }
        let envelope = json!({
            "pivot_prompt": format!(
                "<user_query>\n{}\n</user_query>\n{}\nReview the tool response(s) above. If sufficient information exists to fully answer the user query, synthesize the final response. Otherwise, continue and generate the next tool call as needed.\n",
                ctx.initial_user_query, history
            ),
            "samplers": ctx.samplers,
            "think_mode": &ctx.active_think_mode,
            "reasoning_effort": &ctx.active_reasoning_effort,
            "reasoning_budget_message": &ctx.reasoning_budget_message
        });
        format!("[PIVOT_CONTINUE]{}", envelope)
    }
}

/// The messages of tool steps, in order: the call as an assistant message, the result as a
/// `tool` message (or, for templates without a tool role, a user message holding a
/// `<tool_response>` block). Used for this answer's steps and for earlier turns' recorded ones.
pub fn step_messages(steps: &[RecordedStep], tool_role: bool) -> Vec<(&'static str, String)> {
    steps
        .iter()
        .flat_map(|step| {
            let result = if tool_role {
                ("tool", step.result.clone())
            } else {
                ("user", cluaiz_agents::ToolsEngine::format_tool_response_raw(&step.name, &step.result))
            };
            [("assistant", step.call_xml.clone()), result]
        })
        .collect()
}

/// Whether the model's chat template has its own `tool` role
pub fn template_has_tool_role(chat_template: Option<&str>) -> bool {
    chat_template.is_some_and(template_renders_tool_role)
}

/// What a tool meta-tool call produced
pub struct MetaOutcome {
    /// The tool result for the model
    pub text: String,
    /// A tool the model unloaded: its earlier load results in this answer become tombstones
    pub unloaded: Option<String>,
}

/// Answers the Engine's own tool meta-tools (`find_tools`, `load_tool`, `unload_tool`). In a chat
/// a loaded tool is bound to the session, so the next messages carry it; an unloaded one is
/// unbound and kept out of the suggestions for a few answers.
pub async fn run_meta_tool(
    ctx: &PreparedChatContext,
    turns: &ToolTurns,
    session_id: Option<&str>,
    name: &str,
    payload: &str,
) -> MetaOutcome {
    let args = serde_json::from_str::<Value>(payload).unwrap_or_else(|_| json!({}));
    let text_arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string();
    match name {
        cluaiz_agents::LOAD_TOOL_NAME => {
            let id = text_arg("id");
            let implicit = args.get("implicit").and_then(Value::as_bool) == Some(true);
            let functions: Vec<String> = args
                .get("functions")
                .and_then(Value::as_array)
                .map(|f| f.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
            // Room left for tools in this answer, after what earlier steps added
            let room = ctx.tool_load_room.unwrap_or_default().saturating_sub(turns.chars());
            let loaded = cluaiz_agents::ToolSearch::load(&id, &functions, &ctx.initial_user_query, room).await;
            tracing::info!("📦 [Tools] load_tool(\"{}\") -> {:?}", id, loaded.loaded);
            if let (Some(tool_id), Some(session_id)) = (&loaded.loaded, session_id) {
                // The tool stays in the conversation where it was loaded until the model unloads
                // it (removing it sooner would change an earlier turn and cost a re-process)
                cluaiz_agents::SessionToolManager::update_session_tools(
                    session_id,
                    vec![cluaiz_agents::SessionToolBinding { id: tool_id.clone(), turns: -1, inline: true }],
                    Vec::new(),
                );
            }
            let text = if implicit && loaded.loaded.is_some() {
                format!("'{}' is a tool, not a function, and it was not loaded yet. It is loaded now; call one of its functions.
{}", id, loaded.text)
            } else {
                loaded.text
            };
            MetaOutcome { text, unloaded: None }
        }
        cluaiz_agents::UNLOAD_TOOL_NAME => {
            let id = text_arg("id");
            tracing::info!("📤 [Tools] unload_tool(\"{}\")", id);
            if let Some(session_id) = session_id {
                cluaiz_agents::SessionToolManager::update_session_tools(session_id, Vec::new(), vec![id.clone()]);
                cluaiz_agents::SessionToolManager::suppress(session_id, &id, UNLOADED_TOOL_ANSWERS);
            }
            // Earlier turns that loaded it get a short note instead of its instructions
            if let Some((chat, _)) = &ctx.ledger_turn {
                let mut ledger = cluaiz_agents::ChatLedger::get(chat);
                if ledger.tombstone(&id) {
                    cluaiz_agents::ChatLedger::put(chat, ledger);
                }
            }
            MetaOutcome { text: format!("Tool '{}' unloaded.", id), unloaded: Some(id) }
        }
        _ => {
            let query = Some(text_arg("query")).filter(|q| !q.is_empty()).unwrap_or_else(|| payload.to_string());
            tracing::info!("🔎 [Tools] find_tools(\"{}\")", query);
            MetaOutcome { text: cluaiz_agents::ToolSearch::find(&query, &ctx.loaded_tool_ids), unloaded: None }
        }
    }
}

/// Whether the model called an installed tool by its id although it is not loaded (small
/// models skip `load_tool`). The call is then answered as `load_tool`, so the model gets the
/// tool's functions and calls the right one next.
pub fn needs_load(ctx: &PreparedChatContext, turns: &ToolTurns, session_id: Option<&str>, name: &str) -> bool {
    if ctx.tool_load_room.is_none() || cluaiz_agents::ToolSearch::is_meta_tool(name) {
        return false;
    }
    let loaded = |id: &String| id.eq_ignore_ascii_case(name.trim());
    let bound = session_id
        .map(cluaiz_agents::SessionToolManager::get_active_tool_ids)
        .unwrap_or_default();
    !ctx.loaded_tool_ids.iter().any(loaded)
        && !bound.iter().any(loaded)
        && !turns.loaded_ids().iter().any(loaded)
        && cluaiz_agents::ToolSearch::is_installed_tool_id(name)
}

/// Answers an unloaded tool is kept out of the suggestions
const UNLOADED_TOOL_ANSWERS: u32 = 3;

/// Whether a Jinja chat template has its own branch for `tool` messages (Qwen, Llama 3, Hermes...).
fn template_renders_tool_role(template: &str) -> bool {
    template.contains("\"tool\"") || template.contains("'tool'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_role_detection() {
        assert!(template_renders_tool_role("{%- elif message.role == \"tool\" %}"));
        assert!(template_renders_tool_role("{% if message['role'] == 'tool' %}"));
        assert!(!template_renders_tool_role("{% if message['role'] == 'assistant' %}model{% endif %}"));
    }

    #[test]
    fn test_each_step_is_added_once() {
        let mut turns = ToolTurns::new(Some("message.role == \"tool\""));
        turns.push("{\"name\":\"a\"}", "a", "first");
        turns.push("{\"name\":\"b\"}", "b", "second");
        let messages = turns.messages();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[1]["role"], "tool");
        assert_eq!(messages[1]["content"], "first");
        assert_eq!(messages[3]["content"], "second");
        let all = serde_json::to_string(&messages).unwrap();
        assert_eq!(all.matches("first").count(), 1);
    }

    #[test]
    fn test_templates_without_tool_role_get_a_user_tool_response() {
        let mut turns = ToolTurns::new(Some("{{ message.role }}"));
        turns.push("{\"name\":\"a\"}", "a", "done");
        let messages = turns.messages();
        assert_eq!(messages[1]["role"], "user");
        assert!(messages[1]["content"].as_str().unwrap().starts_with("<tool_response>"));
    }
}
