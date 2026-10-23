//! Tool calls of one answer, added to the conversation the way chat templates expect them.
//!
//! Each step adds one assistant message (the `<tool_call>`) and one tool message (its result),
//! and the whole conversation is rendered again through the model's template. The engine keeps
//! the KV cache of the unchanged beginning, so only the new step is processed, and every result
//! is in the context once (llama-server and the OpenAI API work the same way).

use super::preflight::PreparedChatContext;
use serde_json::{json, Value};

struct ToolStep {
    call_xml: String,
    name: String,
    result: String,
}

pub struct ToolTurns {
    steps: Vec<ToolStep>,
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
        self.steps.push(ToolStep {
            call_xml: format!("<tool_call>\n{}\n</tool_call>", raw_call_json),
            name: name.to_string(),
            result: result.to_string(),
        });
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
        self.steps
            .iter()
            .flat_map(|step| {
                let result = if self.tool_role {
                    json!({ "role": "tool", "content": step.result })
                } else {
                    json!({
                        "role": "user",
                        "content": cluaiz_agents::ToolsEngine::format_tool_response_raw(&step.name, &step.result)
                    })
                };
                [json!({ "role": "assistant", "content": step.call_xml }), result]
            })
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

/// Runs a `tool_search` call: finds the tools for its query and returns their schemas as the
/// tool result. In a chat they are also bound to the session (for the tool's default number
/// of turns), so the next messages have them in the prompt.
pub async fn run_tool_search(ctx: &PreparedChatContext, session_id: Option<&str>, payload: &str) -> String {
    let query = serde_json::from_str::<Value>(payload)
        .ok()
        .and_then(|args| args.get("query").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| payload.to_string());
    let found = cluaiz_agents::ToolSearch::search(
        &query,
        &ctx.tool_search_exclude,
        ctx.tool_search_room.unwrap_or_default(),
    )
    .await;
    tracing::info!("🔎 [ToolSearch] \"{}\" loaded {:?}", query, found.loaded_ids);
    if let Some(session_id) = session_id {
        let bindings = found
            .loaded_ids
            .iter()
            .map(|id| cluaiz_agents::SessionToolBinding {
                id: id.clone(),
                turns: cluaiz_agents::ToolsEngine::get_tool(id)
                    .ok()
                    .flatten()
                    .map_or(-1, |entry| entry.default_turns),
            })
            .collect();
        cluaiz_agents::SessionToolManager::update_session_tools(session_id, bindings, Vec::new());
    }
    found.text
}

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
