//! How each provider differs from the OpenAI Chat Completions format. The known ones are told
//! from the base URL; the `compat` of a provider in providers.json overrides any field. A provider
//! the Engine does not know gets the plain OpenAI format, and a field it refuses is left out and
//! the request sent again (`chat::open`), so an unknown provider still works.
//!
//! Each difference is one field here, used in one place: `request.rs` (what is sent), `history.rs`
//! (how the conversation is written) or `stream.rs` (how the answer is read).

use cluaiz_core::providers::{Provider, Target};

/// The request format a provider speaks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    /// OpenAI Chat Completions (`/chat/completions`)
    OpenAi,
    /// Anthropic Messages (`/messages`; on Vertex AI, `:streamRawPredict`)
    Anthropic,
}

/// The chat URL of a target: `/chat/completions`, `/messages`, or on Vertex AI the model's
/// `:streamRawPredict` (`:rawPredict` when not streamed)
pub fn chat_url(target: &Target, stream: bool) -> String {
    let base = target.config.base_url.trim_end_matches('/');
    match Compat::wire_of(&target.config) {
        Wire::OpenAi => format!("{base}/chat/completions"),
        Wire::Anthropic if Compat::is_vertex(&target.config) => {
            format!(
                "{base}/publishers/anthropic/models/{}:{}",
                target.model,
                if stream {
                    "streamRawPredict"
                } else {
                    "rawPredict"
                }
            )
        }
        Wire::Anthropic => format!("{base}/messages"),
    }
}

/// How a provider turns thinking on and off, and shows it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingFormat {
    /// `reasoning_effort: low | medium | high`; nothing to turn it off
    OpenAi,
    /// Gemini: `extra_body.google.thinking_config` (`include_thoughts`, `thinking_level`) to show
    /// it, `reasoning_effort: "none"` to turn it off; the thinking comes between `<thought>` tags
    Gemini,
    /// `enable_thinking: true | false` (Qwen on DashScope)
    Qwen,
    /// `chat_template_kwargs.enable_thinking` (Qwen on vLLM, SGLang, llama.cpp servers)
    QwenTemplate,
    /// `chat_template_kwargs.enable_thinking: true | false`, with `reasoning_effort` for the level
    /// (NVIDIA's API: Nemotron turns thinking off only by the template, some models think only with
    /// `reasoning_effort`)
    Nvidia,
    /// `thinking.type: enabled | disabled` (z.ai GLM)
    Zai,
    /// `thinking.type: enabled | disabled`, with `reasoning_effort` (DeepSeek)
    DeepSeek,
    /// `reasoning.effort`, `"none"` to turn it off (OpenRouter)
    OpenRouter,
    /// `reasoning.enabled` (Together)
    Together,
    /// Anthropic: `thinking: {type: "adaptive", display: "summarized"}` and
    /// `output_config.effort`; off leaves `thinking` out and asks for low effort
    Anthropic,
    /// Nothing is sent
    None,
}

impl ThinkingFormat {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "openai" => Self::OpenAi,
            "gemini" | "google" => Self::Gemini,
            "qwen" => Self::Qwen,
            "qwen-template" | "chat-template" => Self::QwenTemplate,
            "nvidia" | "nim" => Self::Nvidia,
            "zai" | "glm" => Self::Zai,
            "deepseek" => Self::DeepSeek,
            "openrouter" => Self::OpenRouter,
            "together" => Self::Together,
            "anthropic" | "claude" => Self::Anthropic,
            "none" => Self::None,
            _ => return None,
        })
    }
}

/// The form tool call ids must have
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdForm {
    /// Any id the provider or the Engine made
    Any,
    /// Nine letters and digits (Mistral)
    Alnum9,
}

/// One provider's differences, resolved
#[derive(Debug, Clone, PartialEq)]
pub struct Compat {
    pub wire: Wire,
    /// Claude on Vertex AI: no `model` in the body, `anthropic_version` instead
    pub vertex: bool,
    pub thinking: ThinkingFormat,
    /// Whether `reasoning_effort` is taken
    pub reasoning_effort: bool,
    /// The answer limit's field
    pub max_tokens_field: &'static str,
    /// Whether `stream_options.include_usage` is taken
    pub usage_in_stream: bool,
    /// An assistant message with calls carries `reasoning_content` (DeepSeek wants it back)
    pub reasoning_content_on_calls: bool,
    pub tool_call_ids: IdForm,
    /// An assistant message with calls has `content: ""` instead of null
    pub content_not_null: bool,
}

impl Default for Compat {
    fn default() -> Self {
        Self {
            wire: Wire::OpenAi,
            vertex: false,
            thinking: ThinkingFormat::OpenAi,
            reasoning_effort: true,
            max_tokens_field: "max_tokens",
            usage_in_stream: true,
            reasoning_content_on_calls: false,
            tool_call_ids: IdForm::Any,
            content_not_null: false,
        }
    }
}

impl Compat {
    /// The provider's differences: detected from its URL, then providers.json's `compat`
    pub fn of(target: &Target) -> Self {
        let mut compat = detect(&target.config.base_url);
        if Self::wire_of(&target.config) == Wire::Anthropic {
            compat = Compat {
                wire: Wire::Anthropic,
                vertex: Self::is_vertex(&target.config),
                thinking: ThinkingFormat::Anthropic,
                ..Compat::default()
            };
        }
        let set = &target.config.compat;
        if let Some(format) = set.thinking.as_deref().and_then(ThinkingFormat::parse) {
            compat.thinking = format;
        }
        if let Some(on) = set.reasoning_effort {
            compat.reasoning_effort = on;
        }
        match set.max_tokens_field.as_deref() {
            Some("max_completion_tokens") => compat.max_tokens_field = "max_completion_tokens",
            Some("max_tokens") => compat.max_tokens_field = "max_tokens",
            _ => {}
        }
        if let Some(on) = set.usage_in_stream {
            compat.usage_in_stream = on;
        }
        if let Some(on) = set.reasoning_content_on_calls {
            compat.reasoning_content_on_calls = on;
        }
        match set.tool_call_ids.as_deref() {
            Some("alnum9") => compat.tool_call_ids = IdForm::Alnum9,
            Some("any") => compat.tool_call_ids = IdForm::Any,
            _ => {}
        }
        if let Some(on) = set.content_not_null {
            compat.content_not_null = on;
        }
        compat
    }

    /// The request format: providers.json's `type`, else Anthropic's own URLs (the Claude API,
    /// Claude in Amazon Bedrock, Claude on Vertex AI) speak Anthropic Messages
    pub fn wire_of(provider: &Provider) -> Wire {
        let url = provider.base_url.to_ascii_lowercase();
        let anthropic_url = url.contains("api.anthropic.com")
            || (url.contains("bedrock-mantle") && url.contains("/anthropic"))
            || (url.contains("aiplatform.googleapis.com") && !url.contains("/endpoints/openapi"));
        if provider
            .kind
            .eq_ignore_ascii_case(cluaiz_core::providers::ANTHROPIC)
            || anthropic_url
        {
            Wire::Anthropic
        } else {
            Wire::OpenAi
        }
    }

    /// Claude on Vertex AI (`…/locations/{location}` with the model in the URL)
    pub fn is_vertex(provider: &Provider) -> bool {
        provider
            .base_url
            .to_ascii_lowercase()
            .contains("aiplatform.googleapis.com")
            && Self::wire_of(provider) == Wire::Anthropic
    }

    /// Whether the thinking comes between `<thought>` tags in the answer text (Gemini)
    pub fn thought_tags(&self) -> bool {
        self.thinking == ThinkingFormat::Gemini
    }
}

/// The known providers' differences, from the base URL. The same sources as pi-ai's
/// `detectCompat` and Cline's provider rules, checked against each provider's documentation.
pub fn detect(base_url: &str) -> Compat {
    let url = base_url.to_ascii_lowercase();
    let has = |part: &str| url.contains(part);
    let mut compat = Compat::default();
    if has("generativelanguage.googleapis.com")
        || (has("aiplatform.googleapis.com") && has("openapi"))
    {
        compat.thinking = ThinkingFormat::Gemini;
        // Vertex AI's OpenAI URL refuses `reasoning_effort` (Google AI Studio's takes it)
        compat.reasoning_effort = !has("aiplatform.googleapis.com");
    } else if has("deepseek.com") {
        compat.thinking = ThinkingFormat::DeepSeek;
        compat.reasoning_content_on_calls = true;
    } else if has("api.z.ai") || has("bigmodel.cn") {
        compat.thinking = ThinkingFormat::Zai;
        compat.reasoning_effort = false;
    } else if has("together.ai") || has("together.xyz") {
        compat.thinking = ThinkingFormat::Together;
        compat.reasoning_effort = false;
    } else if has("openrouter.ai") {
        compat.thinking = ThinkingFormat::OpenRouter;
    } else if has("dashscope") && has("aliyuncs.com") {
        compat.thinking = ThinkingFormat::Qwen;
    } else if has("api.mistral.ai") {
        compat.tool_call_ids = IdForm::Alnum9;
        compat.reasoning_effort = false;
    } else if has("integrate.api.nvidia.com") {
        // Checked on NVIDIA's API (2026-11-12): every model takes both fields
        compat.thinking = ThinkingFormat::Nvidia;
    } else if has("api.x.ai") || has("moonshot") {
        compat.reasoning_effort = false;
    } else if has("api.openai.com") || has("openai.azure.com") {
        // OpenAI's reasoning models take only `max_completion_tokens`
        compat.max_tokens_field = "max_completion_tokens";
    }
    compat
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_core::providers::Provider;

    fn target(url: &str) -> Target {
        Target {
            provider: "p".into(),
            config: Provider::new(url),
            model: "m".into(),
        }
    }

    #[test]
    fn known_providers_are_told_from_the_url() {
        assert_eq!(
            Compat::of(&target(
                "https://generativelanguage.googleapis.com/v1beta/openai"
            ))
            .thinking,
            ThinkingFormat::Gemini
        );
        assert!(Compat::of(&target(
            "https://generativelanguage.googleapis.com/v1beta/openai"
        ))
        .thought_tags());
        let deepseek = Compat::of(&target("https://api.deepseek.com/v1"));
        assert_eq!(deepseek.thinking, ThinkingFormat::DeepSeek);
        assert!(deepseek.reasoning_content_on_calls);
        assert_eq!(
            Compat::of(&target("https://api.mistral.ai/v1")).tool_call_ids,
            IdForm::Alnum9
        );
        assert_eq!(
            Compat::of(&target("https://api.openai.com/v1")).max_tokens_field,
            "max_completion_tokens"
        );
        assert_eq!(
            Compat::of(&target("https://openrouter.ai/api/v1")).thinking,
            ThinkingFormat::OpenRouter
        );
        // Groq, a server on this computer, anything unknown: the plain OpenAI format
        assert_eq!(
            Compat::of(&target("https://api.groq.com/openai/v1")),
            Compat::default()
        );
        assert_eq!(
            Compat::of(&target("http://127.0.0.1:8080/v1")),
            Compat::default()
        );
        let nvidia = Compat::of(&target("https://integrate.api.nvidia.com/v1"));
        assert_eq!(nvidia.thinking, ThinkingFormat::Nvidia);
        assert!(nvidia.reasoning_effort);
    }

    #[test]
    fn providers_json_wins_over_the_url() {
        let mut t = target("http://127.0.0.1:8000/v1");
        t.config.compat.thinking = Some("qwen-template".into());
        t.config.compat.max_tokens_field = Some("max_completion_tokens".into());
        t.config.compat.tool_call_ids = Some("alnum9".into());
        let compat = Compat::of(&t);
        assert_eq!(compat.thinking, ThinkingFormat::QwenTemplate);
        assert_eq!(compat.max_tokens_field, "max_completion_tokens");
        assert_eq!(compat.tool_call_ids, IdForm::Alnum9);
        // An unknown name changes nothing
        t.config.compat.thinking = Some("mystery".into());
        assert_eq!(Compat::of(&t).thinking, ThinkingFormat::OpenAi);
    }
}
