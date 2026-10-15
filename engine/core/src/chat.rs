//! Chat request envelope for legacy text and typed message content.
//! Context estimates and trimming currently apply to the existing text request path.

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

    /// Rough token count used by the context rules (4 characters per token, at least 1)
    pub fn estimated_tokens(&self) -> usize {
        (self.content.len() / 4).max(1)
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
        })
    }
}

/// Tokens kept free for the answer when deciding whether the history fits.
pub fn answer_reserve(max_tokens: Option<usize>, n_ctx: usize) -> usize {
    max_tokens.unwrap_or(1024).clamp(256, (n_ctx / 4).max(512))
}

/// Context shifting before a request: when the messages do not fit `n_ctx` minus `reserve`, the oldest
/// turns after the system message are dropped (at least the overflow, or the mode's share of the
/// context when that is larger). The last message always stays. Returns the estimated tokens dropped.
pub fn trim_history(
    messages: &mut Vec<ChatMessage>,
    n_ctx: usize,
    reserve: usize,
    mode: ContextShiftingMode,
) -> usize {
    if messages.len() <= 2 || mode == ContextShiftingMode::Off {
        return 0;
    }
    let budget = n_ctx.saturating_sub(reserve).max(512);
    let total: usize = messages.iter().map(ChatMessage::estimated_tokens).sum();
    if total <= budget {
        return 0;
    }
    let overflow = total - budget;
    let share = |f: f32| ((n_ctx as f32) * f) as usize;
    let target = match mode {
        ContextShiftingMode::Minimal => share(0.05),
        ContextShiftingMode::Standard => share(0.10),
        ContextShiftingMode::Aggressive => share(0.25),
        ContextShiftingMode::Extreme => share(0.50),
        ContextShiftingMode::Auto | ContextShiftingMode::Off => overflow,
    }
    .max(overflow);

    let keep_start = usize::from(
        messages
            .first()
            .map(|m| m.role.eq_ignore_ascii_case("system"))
            .unwrap_or(false),
    );
    let mut dropped = 0;
    while messages.len() > keep_start + 1 && dropped < target {
        dropped += messages.remove(keep_start).estimated_tokens();
    }
    dropped
}

/// Tokens the answer may use: what is left of the context after the prompt (at least 32),
/// never more than the request's `max_tokens`.
pub fn answer_budget(n_ctx: usize, prompt_tokens: usize, max_tokens: Option<usize>) -> usize {
    let free = n_ctx.saturating_sub(prompt_tokens).max(32);
    match max_tokens {
        Some(requested) => requested.min(n_ctx).min(free),
        None => free,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_keeps_system_and_last_message() {
        let long = "x".repeat(4000); // ~1000 tokens each
        let mut msgs = vec![
            ChatMessage::new("system", "be brief"),
            ChatMessage::new("user", long.clone()),
            ChatMessage::new("assistant", long.clone()),
            ChatMessage::new("user", "last"),
        ];
        let dropped = trim_history(&mut msgs, 2048, 512, ContextShiftingMode::Auto);
        assert!(dropped > 0);
        assert_eq!(msgs.first().unwrap().role, "system");
        assert_eq!(msgs.last().unwrap().content, "last");
    }

    #[test]
    fn nothing_trimmed_when_it_fits_or_shifting_is_off() {
        let mut msgs = vec![
            ChatMessage::new("user", "a"),
            ChatMessage::new("assistant", "b"),
            ChatMessage::new("user", "c"),
        ];
        assert_eq!(
            trim_history(&mut msgs, 4096, 1024, ContextShiftingMode::Auto),
            0
        );
        let mut big = vec![ChatMessage::new("user", "x".repeat(40_000)); 3];
        assert_eq!(
            trim_history(&mut big, 4096, 1024, ContextShiftingMode::Off),
            0
        );
        assert_eq!(big.len(), 3);
    }

    #[test]
    fn answer_budget_is_bounded_by_free_context() {
        assert_eq!(answer_budget(4096, 1000, None), 3096);
        assert_eq!(answer_budget(4096, 1000, Some(100)), 100);
        assert_eq!(answer_budget(4096, 5000, None), 32);
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
        };
        let v: serde_json::Value = serde_json::from_str(&env.to_prompt()).unwrap();
        assert_eq!(v["messages"][0]["content"], "hi");
        assert_eq!(v["think_mode"], "auto");
        assert_eq!(v["reasoning_effort"], "low");
        assert!(v["samplers"]["temp"].is_null());
    }
}
