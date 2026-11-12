//! The chat request a provider gets: the Engine's settings (samplers, answer limit, thinking on or
//! off and its effort, tools, answer schema) written the way that provider takes them
//! (`compat.rs`). The server's chat and the terminal build it here, so both send the same.

use cluaiz_core::ThinkingBudget;
use serde_json::{json, Map, Value};

use super::compat::{Compat, ThinkingFormat, Wire};

/// Samplers and the answer limit, as the request, the flags or settings.json give them
#[derive(Debug, Clone, Default)]
pub struct Sampling {
    pub temperature: Option<Value>,
    pub top_p: Option<Value>,
    pub presence_penalty: Option<Value>,
    pub frequency_penalty: Option<Value>,
    pub seed: Option<Value>,
    pub max_tokens: Option<usize>,
}

impl Sampling {
    /// From a request's samplers (llama names: `temp`), else settings.json
    pub fn from_request(samplers: &Value, max_tokens: Option<usize>) -> Self {
        let saved = cluaiz_core::Settings::current().samplers;
        let pick = |name: &str, fallback: Option<Value>| {
            samplers
                .get(name)
                .filter(|v| !v.is_null())
                .cloned()
                .or(fallback)
        };
        Self {
            temperature: pick("temp", saved.temp.map(|v| json!(v))),
            top_p: pick("top_p", saved.top_p.map(|v| json!(v))),
            presence_penalty: pick("presence_penalty", saved.presence_penalty.map(|v| json!(v))),
            frequency_penalty: pick(
                "frequency_penalty",
                saved.frequency_penalty.map(|v| json!(v)),
            ),
            seed: pick("seed", saved.seed.map(|v| json!(v))),
            max_tokens,
        }
    }
}

/// Whether the model thinks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkMode {
    /// Asked not to think (where the provider can turn it off) and nothing shown
    Off,
    /// Asked to think and to show it
    On,
    /// The model decides; what it shows is shown
    Auto,
}

/// A thinking level providers take
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effort {
    Low,
    Medium,
    High,
}

impl Effort {
    pub fn name(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }
}

/// The Engine's thinking settings for one request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thinking {
    pub mode: ThinkMode,
    pub effort: Option<Effort>,
}

impl Thinking {
    /// From the chat's `think_mode` ("on", "off", "auto") and reasoning effort. A budget in
    /// tokens becomes the level closest to it; 0 tokens is thinking off.
    pub fn of(think_mode: &str, budget: ThinkingBudget) -> Self {
        let effort = match budget {
            ThinkingBudget::Low => Some(Effort::Low),
            ThinkingBudget::Medium => Some(Effort::Medium),
            ThinkingBudget::High | ThinkingBudget::Max => Some(Effort::High),
            ThinkingBudget::Tokens(0) | ThinkingBudget::Auto => None,
            ThinkingBudget::Tokens(n) if n <= 2048 => Some(Effort::Low),
            ThinkingBudget::Tokens(n) if n <= 8192 => Some(Effort::Medium),
            ThinkingBudget::Tokens(_) => Some(Effort::High),
        };
        let mode = match think_mode {
            "off" => ThinkMode::Off,
            _ if budget == ThinkingBudget::Tokens(0) => ThinkMode::Off,
            "on" => ThinkMode::On,
            _ => ThinkMode::Auto,
        };
        Self {
            mode,
            effort: if mode == ThinkMode::Off { None } else { effort },
        }
    }

}

/// Takes out of `body` every field that asks the provider not to think (the forms
/// `add_thinking` writes); true when one was there. Used when a model refuses to stop thinking:
/// the request goes again without them, and the model's thinking is shown as it comes.
pub fn drop_thinking_off(body: &mut Value) -> bool {
    let Some(fields) = body.as_object_mut() else {
        return false;
    };
    let mut dropped = false;
    let mut remove_if = |fields: &mut Map<String, Value>, key: &str, off: &dyn Fn(&Value) -> bool| {
        if fields.get(key).is_some_and(off) {
            fields.remove(key);
            dropped = true;
        }
    };
    remove_if(fields, "reasoning_effort", &|v| v == "none");
    remove_if(fields, "enable_thinking", &|v| v == false);
    remove_if(fields, "thinking", &|v| v["type"] == "disabled");
    remove_if(fields, "reasoning", &|v| {
        v["effort"] == "none" || v["enabled"] == false
    });
    if let Some(kwargs) = fields
        .get_mut("chat_template_kwargs")
        .and_then(Value::as_object_mut)
    {
        remove_if(kwargs, "enable_thinking", &|v| v == false);
        if kwargs.is_empty() {
            fields.remove("chat_template_kwargs");
        }
    }
    dropped
}

/// Fields the Engine adds on its own: a provider that refuses one gets the request again without
/// it (`chat::open`). The conversation, tools and answer schema are never left out.
pub const OPTIONAL_FIELDS: [&str; 14] = [
    "stream_options",
    "output_config",
    "reasoning_effort",
    "extra_body",
    "thinking",
    "enable_thinking",
    "chat_template_kwargs",
    "reasoning",
    "max_completion_tokens",
    "seed",
    "presence_penalty",
    "frequency_penalty",
    "top_p",
    "temperature",
];

/// What a chat request holds, before it is written for a provider
pub struct ChatParts<'a> {
    pub model: &'a str,
    pub messages: Vec<Value>,
    pub sampling: &'a Sampling,
    pub thinking: Thinking,
    pub tools: Option<&'a Value>,
    pub response_format: Option<&'a Value>,
    /// The request's own OpenAI fields (`stop`, `user`, …): written last, so a field the person
    /// sent wins
    pub extra: Option<&'a Map<String, Value>>,
}

/// The streamed chat request, written for the provider `compat` describes
pub fn chat_body(compat: &Compat, parts: ChatParts) -> Value {
    if compat.wire == Wire::Anthropic {
        return super::anthropic::body(compat, parts);
    }
    let ChatParts {
        model,
        messages,
        sampling,
        thinking,
        tools,
        response_format,
        extra,
    } = parts;
    let mut body = json!({ "model": model, "messages": messages, "stream": true });
    if compat.usage_in_stream {
        body["stream_options"] = json!({ "include_usage": true });
    }
    for (field, value) in [
        ("temperature", &sampling.temperature),
        ("top_p", &sampling.top_p),
        ("presence_penalty", &sampling.presence_penalty),
        ("frequency_penalty", &sampling.frequency_penalty),
        ("seed", &sampling.seed),
    ] {
        if let Some(value) = value {
            body[field] = value.clone();
        }
    }
    if let Some(max) = sampling.max_tokens {
        body[compat.max_tokens_field] = json!(max);
    }
    if let Some(tools) = tools.filter(|t| t.as_array().is_some_and(|a| !a.is_empty())) {
        body["tools"] = tools.clone();
    }
    if let Some(format) = response_format.filter(|v| !v.is_null()) {
        body["response_format"] = format.clone();
    }
    add_thinking(&mut body, compat, thinking);
    for (field, value) in extra.into_iter().flatten() {
        body[field] = value.clone();
    }
    body
}

/// Thinking on, off and its level, in the provider's own fields
fn add_thinking(body: &mut Value, compat: &Compat, thinking: Thinking) {
    let effort = thinking.effort.map(Effort::name);
    let effort_field = |body: &mut Value| {
        if let (Some(effort), true) = (effort, compat.reasoning_effort) {
            body["reasoning_effort"] = json!(effort);
        }
    };
    match (compat.thinking, thinking.mode) {
        (ThinkingFormat::None, _) | (ThinkingFormat::Anthropic, _) => {}
        (ThinkingFormat::OpenAi, ThinkMode::Off) => {}
        (ThinkingFormat::OpenAi, _) => effort_field(body),
        // Gemini takes either `reasoning_effort` or `thinking_config`, never both
        (ThinkingFormat::Gemini, ThinkMode::Off) => {
            if compat.reasoning_effort {
                body["reasoning_effort"] = json!("none");
            }
        }
        (ThinkingFormat::Gemini, _) => {
            let mut config = json!({ "include_thoughts": true });
            if let Some(effort) = effort {
                config["thinking_level"] = json!(effort);
            }
            body["extra_body"] = json!({ "google": { "thinking_config": config } });
        }
        (ThinkingFormat::Qwen, ThinkMode::Auto) => {}
        (ThinkingFormat::Qwen, mode) => body["enable_thinking"] = json!(mode == ThinkMode::On),
        (ThinkingFormat::QwenTemplate, ThinkMode::Auto) => {}
        (ThinkingFormat::QwenTemplate, mode) => {
            body["chat_template_kwargs"] = json!({ "enable_thinking": mode == ThinkMode::On })
        }
        (ThinkingFormat::Nvidia, ThinkMode::Off) => {
            body["chat_template_kwargs"] = json!({ "enable_thinking": false })
        }
        (ThinkingFormat::Nvidia, mode) => {
            if mode == ThinkMode::On {
                body["chat_template_kwargs"] = json!({ "enable_thinking": true });
            }
            effort_field(body);
        }
        (ThinkingFormat::Zai, ThinkMode::Auto) => {}
        (ThinkingFormat::Zai, mode) => {
            body["thinking"] =
                json!({ "type": if mode == ThinkMode::On { "enabled" } else { "disabled" } })
        }
        (ThinkingFormat::DeepSeek, mode) => {
            match mode {
                ThinkMode::Off => body["thinking"] = json!({ "type": "disabled" }),
                ThinkMode::On => body["thinking"] = json!({ "type": "enabled" }),
                ThinkMode::Auto => {}
            }
            effort_field(body);
        }
        (ThinkingFormat::OpenRouter, ThinkMode::Off) => {
            body["reasoning"] = json!({ "effort": "none" })
        }
        (ThinkingFormat::OpenRouter, mode) => match effort {
            Some(effort) => body["reasoning"] = json!({ "effort": effort }),
            None if mode == ThinkMode::On => body["reasoning"] = json!({ "enabled": true }),
            None => {}
        },
        (ThinkingFormat::Together, ThinkMode::Auto) => {}
        (ThinkingFormat::Together, mode) => {
            body["reasoning"] = json!({ "enabled": mode == ThinkMode::On })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(format: ThinkingFormat, mode: &str, budget: ThinkingBudget) -> Value {
        let compat = Compat {
            thinking: format,
            ..Compat::default()
        };
        chat_body(
            &compat,
            ChatParts {
                model: "m",
                messages: vec![],
                sampling: &Sampling::default(),
                thinking: Thinking::of(mode, budget),
                tools: None,
                response_format: None,
                extra: None,
            },
        )
    }

    #[test]
    fn thinking_on_off_and_its_level_reach_each_provider_in_its_own_fields() {
        // OpenAI and the like: the level; off sends nothing (it cannot be turned off there)
        assert_eq!(
            body(ThinkingFormat::OpenAi, "auto", ThinkingBudget::Medium)["reasoning_effort"],
            "medium"
        );
        assert!(body(ThinkingFormat::OpenAi, "off", ThinkingBudget::High)
            .get("reasoning_effort")
            .is_none());
        // NVIDIA: the template turns it on or off, the level rides along when on
        let on = body(ThinkingFormat::Nvidia, "on", ThinkingBudget::High);
        assert_eq!(on["chat_template_kwargs"]["enable_thinking"], true);
        assert_eq!(on["reasoning_effort"], "high");
        let off = body(ThinkingFormat::Nvidia, "off", ThinkingBudget::High);
        assert_eq!(off["chat_template_kwargs"]["enable_thinking"], false);
        assert!(off.get("reasoning_effort").is_none());
        assert!(body(ThinkingFormat::Nvidia, "auto", ThinkingBudget::Auto)
            .get("chat_template_kwargs")
            .is_none());
        // Gemini: shown with its level, or turned off; never both fields
        let on = body(ThinkingFormat::Gemini, "on", ThinkingBudget::High);
        assert_eq!(
            on["extra_body"]["google"]["thinking_config"],
            json!({ "include_thoughts": true, "thinking_level": "high" })
        );
        assert!(on.get("reasoning_effort").is_none());
        assert_eq!(
            body(ThinkingFormat::Gemini, "auto", ThinkingBudget::Auto)["extra_body"]["google"]
                ["thinking_config"],
            json!({ "include_thoughts": true })
        );
        let off = body(ThinkingFormat::Gemini, "off", ThinkingBudget::High);
        assert_eq!(off["reasoning_effort"], "none");
        assert!(off.get("extra_body").is_none());
        // The switches of the others
        assert_eq!(
            body(ThinkingFormat::Qwen, "off", ThinkingBudget::Auto)["enable_thinking"],
            false
        );
        assert!(body(ThinkingFormat::Qwen, "auto", ThinkingBudget::Auto)
            .get("enable_thinking")
            .is_none());
        assert_eq!(
            body(ThinkingFormat::QwenTemplate, "on", ThinkingBudget::Auto)["chat_template_kwargs"]
                ["enable_thinking"],
            true
        );
        assert_eq!(
            body(ThinkingFormat::Zai, "off", ThinkingBudget::Auto)["thinking"]["type"],
            "disabled"
        );
        let deepseek = body(ThinkingFormat::DeepSeek, "on", ThinkingBudget::Low);
        assert_eq!(
            (
                deepseek["thinking"]["type"].as_str(),
                deepseek["reasoning_effort"].as_str()
            ),
            (Some("enabled"), Some("low"))
        );
        assert_eq!(
            body(ThinkingFormat::OpenRouter, "off", ThinkingBudget::High)["reasoning"]["effort"],
            "none"
        );
        assert_eq!(
            body(ThinkingFormat::OpenRouter, "on", ThinkingBudget::Auto)["reasoning"]["enabled"],
            true
        );
        assert_eq!(
            body(ThinkingFormat::Together, "on", ThinkingBudget::Auto)["reasoning"]["enabled"],
            true
        );
        // None: nothing at all
        let none = body(ThinkingFormat::None, "on", ThinkingBudget::High);
        assert!([
            "reasoning_effort",
            "extra_body",
            "thinking",
            "reasoning",
            "enable_thinking"
        ]
        .iter()
        .all(|f| none.get(*f).is_none()));
        // A budget of 0 tokens is thinking off; a budget in tokens becomes a level
        assert_eq!(
            Thinking::of("auto", ThinkingBudget::Tokens(0)).mode,
            ThinkMode::Off
        );
        assert_eq!(
            Thinking::of("auto", ThinkingBudget::Tokens(4000)).effort,
            Some(Effort::Medium)
        );
        assert_eq!(Thinking::of("off", ThinkingBudget::High).effort, None);
    }

    #[test]
    fn every_form_of_thinking_off_can_be_taken_out_again() {
        for format in [
            ThinkingFormat::Gemini,
            ThinkingFormat::Qwen,
            ThinkingFormat::QwenTemplate,
            ThinkingFormat::Nvidia,
            ThinkingFormat::Zai,
            ThinkingFormat::DeepSeek,
            ThinkingFormat::OpenRouter,
            ThinkingFormat::Together,
        ] {
            let compat = Compat {
                thinking: format,
                reasoning_effort: true,
                ..Compat::default()
            };
            let sampling = Sampling::default();
            let parts = |mode| ChatParts {
                model: "m",
                messages: vec![],
                sampling: &sampling,
                thinking: Thinking::of(mode, ThinkingBudget::Auto),
                tools: None,
                response_format: None,
                extra: None,
            };
            let mut off = chat_body(&compat, parts("off"));
            assert!(drop_thinking_off(&mut off), "{format:?}");
            // Gemini's auto also asks for the thoughts to be shown; the others send nothing
            if format != ThinkingFormat::Gemini {
                assert_eq!(off, chat_body(&compat, parts("auto")), "{format:?}");
            }
            for field in ["reasoning_effort", "enable_thinking", "thinking", "reasoning", "chat_template_kwargs"] {
                assert!(off.get(field).is_none(), "{format:?} {field}");
            }
            assert!(!drop_thinking_off(&mut off), "{format:?}");
        }
        // Thinking asked for is kept
        let mut on = body(ThinkingFormat::Nvidia, "on", ThinkingBudget::High);
        assert!(!drop_thinking_off(&mut on));
        assert_eq!(on["chat_template_kwargs"]["enable_thinking"], true);
    }

    #[test]
    fn samplers_limit_and_the_requests_own_fields() {
        let compat = Compat {
            max_tokens_field: "max_completion_tokens",
            usage_in_stream: false,
            ..Compat::default()
        };
        let sampling = Sampling {
            temperature: Some(json!(0.2)),
            seed: Some(json!(7)),
            max_tokens: Some(100),
            ..Sampling::default()
        };
        let mut extra = Map::new();
        extra.insert("stop".into(), json!(["END"]));
        extra.insert("temperature".into(), json!(0.9));
        let tools = json!([{ "type": "function", "function": { "name": "f" } }]);
        let parts = ChatParts {
            model: "m",
            messages: vec![json!({ "role": "user", "content": "hi" })],
            sampling: &sampling,
            thinking: Thinking::of("auto", ThinkingBudget::Auto),
            tools: Some(&tools),
            response_format: None,
            extra: Some(&extra),
        };
        let body = chat_body(&compat, parts);
        assert_eq!(body["max_completion_tokens"], 100);
        assert!(body.get("max_tokens").is_none() && body.get("stream_options").is_none());
        assert_eq!(body["seed"], 7);
        assert_eq!(body["temperature"], 0.9, "the request's own field wins");
        assert_eq!(body["stop"], json!(["END"]));
        assert_eq!(body["tools"][0]["function"]["name"], "f");
        assert_eq!(body["stream"], true);
        // Empty tools are not sent
        let empty = json!([]);
        let body = chat_body(
            &Compat::default(),
            ChatParts {
                model: "m",
                messages: vec![],
                sampling: &Sampling::default(),
                thinking: Thinking::of("auto", ThinkingBudget::Auto),
                tools: Some(&empty),
                response_format: None,
                extra: None,
            },
        );
        assert!(body.get("tools").is_none());
        assert_eq!(body["stream_options"]["include_usage"], true);
    }
}
