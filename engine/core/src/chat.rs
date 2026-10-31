//! Chat request envelope for legacy text and typed message content, and the context rules the
//! engine applies with the model's real token counts.

use crate::settings::{ContextShiftingMode, SamplerDefaults, ThinkingBudget};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage<Content = String> {
    pub role: String,
    pub content: Content,
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
        }
    }
}

/// Shared generation options and messages. The default text form uses `to_prompt`;
/// prepared media stays typed inside the process for its backend consumer.
#[derive(Debug, Clone, Serialize)]
pub struct ChatEnvelope<Content = String> {
    pub messages: Vec<ChatMessage<Content>>,
    /// Values sent with this request; null = the user's setting, then the model's default, then llama.cpp's
    pub samplers: SamplerDefaults,
    /// "on", "off" or "auto" (`FeatureState::name`)
    pub think_mode: String,
    pub reasoning_effort: ThinkingBudget,
    pub reasoning_budget_message: Option<String>,
    pub chat_template_kwargs: Option<serde_json::Value>,
    pub grammar: Option<String>,
    pub response_format: Option<serde_json::Value>,
    /// Native tool schemas survive text/media conversion; omitted for callers without tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
}

impl ChatEnvelope {
    pub fn to_prompt(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

impl<Content> ChatEnvelope<Content> {
    /// Changes message representation while retaining all request generation options.
    /// Typed media envelopes deliberately have no legacy string `to_prompt` method.
    pub async fn try_map_messages<Next, Error, Future>(
        self,
        transform: impl FnOnce(Vec<ChatMessage<Content>>) -> Future,
    ) -> Result<ChatEnvelope<Next>, Error>
    where
        Future: std::future::Future<Output = Result<Vec<ChatMessage<Next>>, Error>>,
    {
        let messages = transform(self.messages).await?;
        Ok(ChatEnvelope {
            messages,
            samplers: self.samplers,
            think_mode: self.think_mode,
            reasoning_effort: self.reasoning_effort,
            reasoning_budget_message: self.reasoning_budget_message,
            chat_template_kwargs: self.chat_template_kwargs,
            grammar: self.grammar,
            response_format: self.response_format,
            tools: self.tools,
        })
    }
}

/// How the answer is constrained: a GBNF grammar as given, or a JSON schema (llama-server's
/// `grammar` and `response_format`)
#[derive(Debug, Clone, PartialEq)]
pub enum OutputConstraint {
    Gbnf(String),
    JsonSchema(serde_json::Value),
}

impl OutputConstraint {
    /// From a request's `grammar` (wins) and `response_format`: `{"type": "json_object"}` (any
    /// object, or its `schema`), `{"type": "json_schema", "json_schema": {"schema": …}}` (OpenAI),
    /// also with the schema directly under `json_schema`. `text` and unknown types: none.
    pub fn from_request(
        grammar: Option<&str>,
        response_format: Option<&serde_json::Value>,
    ) -> Option<Self> {
        if let Some(grammar) = grammar.filter(|g| !g.trim().is_empty()) {
            return Some(Self::Gbnf(grammar.to_string()));
        }
        let format = response_format?;
        match format.get("type").and_then(|t| t.as_str()) {
            Some("json_object") => Some(Self::JsonSchema(
                format
                    .get("schema")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({ "type": "object" })),
            )),
            Some("json_schema") => {
                let spec = format.get("json_schema")?;
                Some(Self::JsonSchema(
                    spec.get("schema").cloned().unwrap_or_else(|| spec.clone()),
                ))
            }
            _ => None,
        }
    }
}

/// Tokens kept free for the answer when a prompt is fitted into the context: the request's
/// `max_tokens` (callers without one pass `n_ctx`), at most a quarter of the context (256..2048).
pub fn answer_reserve(max_tokens: usize, n_ctx: usize) -> usize {
    max_tokens.min((n_ctx / 4).clamp(256, 2048)).max(1)
}

/// Share of the context a mode frees at least each time; `None` = Off or Auto (decided per case).
fn mode_share(mode: ContextShiftingMode, n_ctx: usize) -> Option<usize> {
    let share = |f: f64| (n_ctx as f64 * f) as usize;
    match mode {
        ContextShiftingMode::Minimal => Some(share(0.05)),
        ContextShiftingMode::Standard => Some(share(0.10)),
        ContextShiftingMode::Aggressive => Some(share(0.25)),
        ContextShiftingMode::Extreme => Some(share(0.50)),
        ContextShiftingMode::Auto | ContextShiftingMode::Off => None,
    }
}

/// Prompt tokens to free when a conversation does not fit (`overflow` > 0); `None` = Off, where the
/// request fails like llama-server's "exceeds the available context size". Auto frees only the
/// overflow when the KV cache can shift (`llama_memory_can_shift`: the rest of the old prompt is then
/// reused without a new prefill), else a quarter of the context so the next turns reuse the cache.
pub fn tokens_to_free(
    mode: ContextShiftingMode,
    n_ctx: usize,
    overflow: usize,
    can_shift: bool,
) -> Option<usize> {
    let at_least = match mode {
        ContextShiftingMode::Off => return None,
        ContextShiftingMode::Auto if can_shift => 0,
        ContextShiftingMode::Auto => n_ctx / 4,
        _ => mode_share(mode, n_ctx).unwrap_or(0),
    };
    Some(at_least.max(overflow))
}

/// Tokens a context shift discards during generation once the context is full (`n_left` = tokens
/// after the kept start). `None` = Off: generation stops like llama-server without `--context-shift`.
/// Auto discards half, llama-server's default `n_discard`.
pub fn shift_discard(mode: ContextShiftingMode, n_ctx: usize, n_left: usize) -> Option<usize> {
    if mode == ContextShiftingMode::Off || n_left < 2 {
        return None;
    }
    let n_discard = mode_share(mode, n_ctx).unwrap_or(n_left / 2);
    Some(n_discard.clamp(1, n_left - 1))
}

/// Oldest messages to drop so at least `target` tokens are freed. Whole turns go: dropping starts after
/// the system message and stops at a user message, so the kept history never opens with an answer or a
/// tool result. The last message always stays. `messages` = (role, tokens); returns (messages, tokens).
pub fn oldest_turns_to_drop(messages: &[(&str, usize)], target: usize) -> (usize, usize) {
    let start = usize::from(
        messages
            .first()
            .is_some_and(|(role, _)| role.eq_ignore_ascii_case("system")),
    );
    let last = messages.len().saturating_sub(1);
    let (mut end, mut freed) = (start, 0);
    while end < last && (freed < target || !messages[end].0.eq_ignore_ascii_case("user")) {
        freed += messages[end].1;
        end += 1;
    }
    (end.saturating_sub(start), freed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_constraints_follow_llama_server() {
        use serde_json::json;
        assert_eq!(
            OutputConstraint::from_request(
                Some("root ::= \"a\""),
                Some(&json!({"type": "json_object"}))
            ),
            Some(OutputConstraint::Gbnf("root ::= \"a\"".into()))
        );
        assert_eq!(
            OutputConstraint::from_request(None, Some(&json!({"type": "json_object"}))),
            Some(OutputConstraint::JsonSchema(json!({"type": "object"})))
        );
        let schema = json!({"type": "object", "properties": {"title": {"type": "string"}}});
        assert_eq!(
            OutputConstraint::from_request(
                None,
                Some(
                    &json!({"type": "json_schema", "json_schema": {"name": "card", "schema": schema}})
                )
            ),
            Some(OutputConstraint::JsonSchema(schema.clone()))
        );
        assert_eq!(
            OutputConstraint::from_request(
                None,
                Some(&json!({"type": "json_schema", "json_schema": schema}))
            ),
            Some(OutputConstraint::JsonSchema(schema))
        );
        assert_eq!(
            OutputConstraint::from_request(Some("  "), Some(&json!({"type": "text"}))),
            None
        );
        assert_eq!(OutputConstraint::from_request(None, None), None);
    }

    #[test]
    fn whole_turns_are_dropped_after_the_system_message() {
        let msgs = [
            ("system", 50),
            ("user", 1000),
            ("assistant", 1000),
            ("user", 300),
            ("assistant", 300),
            ("user", 10),
        ];
        // 1 token over: the first user turn and its answer go together
        assert_eq!(oldest_turns_to_drop(&msgs, 1), (2, 2000));
        // More than one turn: dropping stops at the next user message
        assert_eq!(oldest_turns_to_drop(&msgs, 2100), (4, 2600));
        // Never the last message
        assert_eq!(oldest_turns_to_drop(&msgs, 100_000), (4, 2600));
    }

    #[test]
    fn a_tool_result_is_never_left_without_its_call() {
        let msgs = [
            ("user", 10),
            ("assistant", 10),
            ("tool", 500),
            ("assistant", 10),
            ("user", 5),
        ];
        assert_eq!(oldest_turns_to_drop(&msgs, 15), (4, 530));
    }

    #[test]
    fn modes_choose_how_much_to_free() {
        use ContextShiftingMode::*;
        assert_eq!(tokens_to_free(Off, 4096, 100, true), None);
        assert_eq!(tokens_to_free(Auto, 4096, 100, true), Some(100));
        assert_eq!(tokens_to_free(Auto, 4096, 100, false), Some(1024));
        assert_eq!(tokens_to_free(Minimal, 4096, 100, true), Some(204));
        assert_eq!(tokens_to_free(Extreme, 4096, 100, true), Some(2048));
        assert_eq!(tokens_to_free(Minimal, 4096, 3000, true), Some(3000));
    }

    #[test]
    fn shift_discards_like_llama_server() {
        use ContextShiftingMode::*;
        assert_eq!(shift_discard(Off, 4096, 4000), None);
        assert_eq!(shift_discard(Auto, 4096, 4000), Some(2000));
        assert_eq!(shift_discard(Standard, 4096, 4000), Some(409));
        assert_eq!(shift_discard(Extreme, 4096, 1000), Some(999));
        assert_eq!(shift_discard(Auto, 4096, 1), None);
    }

    #[test]
    fn answer_reserve_follows_max_tokens_within_a_quarter() {
        assert_eq!(answer_reserve(100, 4096), 100);
        assert_eq!(answer_reserve(4096, 4096), 1024);
        assert_eq!(answer_reserve(32768, 32768), 2048);
        assert_eq!(answer_reserve(2048, 512), 256);
    }

    #[test]
    fn envelope_has_the_engine_keys() {
        let env = ChatEnvelope {
            messages: vec![ChatMessage::new("user", "hi")],
            samplers: SamplerDefaults::default(),
            think_mode: "auto".into(),
            reasoning_effort: ThinkingBudget::Low,
            reasoning_budget_message: None,
            chat_template_kwargs: None,
            grammar: None,
            response_format: None,
            tools: None,
        };
        let v: serde_json::Value = serde_json::from_str(&env.to_prompt()).unwrap();
        assert_eq!(v["messages"][0]["content"], "hi");
        assert_eq!(v["think_mode"], "auto");
        assert_eq!(v["reasoning_effort"], "low");
        assert!(v["samplers"]["temp"].is_null());
        assert!(v.get("tools").is_none());
        let mut with_tools = env;
        with_tools.tools =
            Some(serde_json::json!([{"type":"function","function":{"name":"read_file"}}]));
        let v: serde_json::Value = serde_json::from_str(&with_tools.to_prompt()).unwrap();
        assert_eq!(v["tools"][0]["function"]["name"], "read_file");
    }
}
