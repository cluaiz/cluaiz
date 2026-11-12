//! The Anthropic Messages format (Claude on the Claude API, Claude in Amazon Bedrock and Claude on
//! Vertex AI): the request written from the Engine's OpenAI-form conversation, and the streamed
//! answer read back into the Engine's thinking, text and calls.
//!
//! Differences from the OpenAI format handled here: the system prompt is a field of its own; a
//! call is a `tool_use` block and its result a `tool_result` block in a user message; results of
//! one answer go back in one user message; `max_tokens` is required; thinking is a `thinking`
//! field with `output_config.effort`, and its blocks come back with their `signature`, which must
//! be sent back unchanged before the calls of that answer. Sampling fields (`temperature`,
//! `top_p`) are not sent: API version 2026-10-21 refuses them.

use serde_json::{json, Map, Value};

use super::compat::Compat;
use super::request::{ChatParts, ThinkMode};

/// Claude on Vertex AI takes the version in the body
pub const VERTEX_VERSION: &str = "vertex-2023-10-16";
/// `max_tokens` when the request sets none (it is required); a model with a lower limit refuses
/// it and the request is sent again with `SMALL_MAX_TOKENS`
pub const DEFAULT_MAX_TOKENS: usize = 32_000;
pub const SMALL_MAX_TOKENS: usize = 8_192;
/// The field of a call (in the Engine's OpenAI form) that carries the thinking blocks of its answer
pub const THINKING_FIELD: &str = "anthropic_thinking";

/// The streamed Messages request
pub fn body(compat: &Compat, parts: ChatParts) -> Value {
    let ChatParts {
        model,
        messages,
        sampling,
        thinking,
        tools,
        response_format,
        extra,
    } = parts;
    let (system, messages) = convert(messages);
    let mut body = json!({
        "max_tokens": sampling.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "messages": messages,
        "stream": true,
    });
    if compat.vertex {
        body["anthropic_version"] = json!(VERTEX_VERSION);
    } else {
        body["model"] = json!(model);
    }
    if let Some(system) = system {
        body["system"] = json!(system);
    }
    let tools: Vec<Value> = tools
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            let function = tool.get("function").unwrap_or(tool);
            let name = function["name"].as_str()?;
            let mut out = json!({ "name": name, "input_schema": function.get("parameters").filter(|p| p.is_object()).cloned().unwrap_or_else(|| json!({ "type": "object" })) });
            if let Some(description) = function["description"].as_str().filter(|d| !d.is_empty()) {
                out["description"] = json!(description);
            }
            Some(out)
        })
        .collect();
    if !tools.is_empty() {
        body["tools"] = json!(tools);
    }
    // Thinking: shown (summarized) unless off; off leaves it out, which turns it off where a
    // model can (Opus 4.8 and earlier) and asks the others for the least effort
    let mut output_config = Map::new();
    match thinking.mode {
        ThinkMode::Off => {
            output_config.insert("effort".into(), json!("low"));
        }
        ThinkMode::On | ThinkMode::Auto => {
            body["thinking"] = json!({ "type": "adaptive", "display": "summarized" });
            if let Some(effort) = thinking.effort {
                output_config.insert("effort".into(), json!(effort.name()));
            }
        }
    }
    // An answer schema (OpenAI `response_format` with `json_schema`) as structured output
    if let Some(schema) = response_format
        .filter(|f| f["type"] == "json_schema")
        .and_then(|f| f.pointer("/json_schema/schema"))
    {
        output_config.insert(
            "format".into(),
            json!({ "type": "json_schema", "schema": schema }),
        );
    }
    if !output_config.is_empty() {
        body["output_config"] = Value::Object(output_config);
    }
    // The request's own fields that have an Anthropic name
    for (field, value) in extra.into_iter().flatten() {
        match field.as_str() {
            "stop" => {
                body["stop_sequences"] = match value {
                    Value::String(one) => json!([one]),
                    other => other.clone(),
                };
            }
            "user" => body["metadata"] = json!({ "user_id": value }),
            _ => {}
        }
    }
    body
}

/// A call id as Anthropic takes it: letters, digits, `_` and `-`, at most 64
fn tool_id(id: &str) -> String {
    let clean: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if clean.is_empty() {
        "call".into()
    } else {
        clean
    }
}

/// OpenAI content (a string or parts) as Anthropic blocks
fn blocks_of(content: &Value) -> Vec<Value> {
    match content {
        Value::String(text) if !text.trim().is_empty() => vec![json!({ "type": "text", "text": text })],
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| match part["type"].as_str() {
                Some("text") => part["text"].as_str().filter(|t| !t.trim().is_empty()).map(|t| json!({ "type": "text", "text": t })),
                Some("image_url") => {
                    let url = part.pointer("/image_url/url").and_then(Value::as_str)?;
                    match url.strip_prefix("data:").and_then(|rest| rest.split_once(";base64,")) {
                        Some((media_type, data)) => Some(json!({ "type": "image", "source": { "type": "base64", "media_type": media_type, "data": data } })),
                        None => Some(json!({ "type": "image", "source": { "type": "url", "url": url } })),
                    }
                }
                Some("input_audio") => Some(json!({ "type": "text", "text": "[An audio attachment was left out: this model does not take audio.]" })),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The Engine's OpenAI-form messages as Anthropic messages: the system prompt apart, calls as
/// `tool_use` (after the thinking blocks of their answer), results as `tool_result` blocks of one
/// user message, messages of the same role joined, and a user message first
pub fn convert(messages: Vec<Value>) -> (Option<String>, Vec<Value>) {
    let mut system: Vec<String> = Vec::new();
    let mut out: Vec<(String, Vec<Value>)> = Vec::new();
    let mut started = false;
    let push = |out: &mut Vec<(String, Vec<Value>)>, role: &str, blocks: Vec<Value>| {
        if blocks.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((last, existing)) if last == role => {
                // Results answer the calls, so they come before other text of the same turn
                if role == "user" && blocks.iter().all(|b| b["type"] == "tool_result") {
                    let at = existing
                        .iter()
                        .position(|b| b["type"] != "tool_result")
                        .unwrap_or(existing.len());
                    for (offset, block) in blocks.into_iter().enumerate() {
                        existing.insert(at + offset, block);
                    }
                } else {
                    existing.extend(blocks);
                }
            }
            _ => out.push((role.to_string(), blocks)),
        }
    };
    for message in messages {
        let role = message["role"].as_str().unwrap_or("user");
        match role {
            "system" | "developer" if !started => {
                if let Some(text) = message["content"].as_str().filter(|t| !t.trim().is_empty()) {
                    system.push(text.to_string());
                } else {
                    system.extend(
                        blocks_of(&message["content"])
                            .iter()
                            .filter_map(|b| b["text"].as_str().map(str::to_owned)),
                    );
                }
            }
            "system" | "developer" => {
                // An instruction later in the chat (such as the Engine's reminder of how tools
                // are loaded) goes to the model as user text
                let text = blocks_of(&message["content"])
                    .iter()
                    .filter_map(|b| b["text"].as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    push(
                        &mut out,
                        "user",
                        vec![json!({ "type": "text", "text": format!("[System note] {text}") })],
                    );
                }
            }
            "assistant" => {
                started = true;
                let mut blocks = Vec::new();
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    if let Some(thinking) = call[THINKING_FIELD].as_array() {
                        blocks.extend(thinking.iter().cloned());
                    }
                }
                blocks.extend(blocks_of(&message["content"]));
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    let input = match &call["function"]["arguments"] {
                        Value::String(text) => serde_json::from_str::<Value>(text)
                            .ok()
                            .filter(Value::is_object)
                            .unwrap_or_else(|| json!({})),
                        value if value.is_object() => value.clone(),
                        _ => json!({}),
                    };
                    blocks.push(json!({ "type": "tool_use", "id": tool_id(call["id"].as_str().unwrap_or("call")), "name": call["function"]["name"], "input": input }));
                }
                push(&mut out, "assistant", blocks);
            }
            "tool" => {
                started = true;
                let content = match &message["content"] {
                    Value::String(text) if !text.is_empty() => text.clone(),
                    Value::String(_) | Value::Null => "(no output)".to_string(),
                    other => other.to_string(),
                };
                push(
                    &mut out,
                    "user",
                    vec![
                        json!({ "type": "tool_result", "tool_use_id": tool_id(message["tool_call_id"].as_str().unwrap_or("call")), "content": content }),
                    ],
                );
            }
            _ => {
                started = true;
                push(&mut out, "user", blocks_of(&message["content"]));
            }
        }
    }
    if out.first().is_some_and(|(role, _)| role != "user") {
        out.insert(
            0,
            (
                "user".into(),
                vec![json!({ "type": "text", "text": "(continue)" })],
            ),
        );
    }
    let messages = out
        .into_iter()
        .map(|(role, content)| json!({ "role": role, "content": content }))
        .collect();
    let system = (!system.is_empty()).then(|| system.join("\n\n"));
    (system, messages)
}

/// Reads an Anthropic Messages stream into the shapes `stream::Decoder` passes on: thinking,
/// text, OpenAI-form call pieces, the thinking blocks to send back, usage and the stop reason
#[derive(Default)]
pub struct MessagesStream {
    /// The kind of each content block, by index
    kinds: std::collections::HashMap<u64, String>,
    /// Thinking blocks of this answer, with their signatures, as they must be sent back
    pub thinking_blocks: Vec<Value>,
    pub prompt_tokens: usize,
    pub output_tokens: usize,
    pub stop: Option<String>,
}

/// One Anthropic event, in the decoder's terms
pub enum Event {
    Nothing,
    Piece {
        thinking: Option<String>,
        text: Option<String>,
        call: Option<Value>,
    },
    Done,
    Error(String),
}

impl MessagesStream {
    pub fn read(&mut self, event: &Value) -> Event {
        let index = event["index"].as_u64().unwrap_or(0);
        match event["type"].as_str() {
            Some("message_start") => {
                let usage = &event["message"]["usage"];
                self.prompt_tokens = [
                    "input_tokens",
                    "cache_creation_input_tokens",
                    "cache_read_input_tokens",
                ]
                .iter()
                .map(|f| usage[*f].as_u64().unwrap_or(0) as usize)
                .sum();
                self.output_tokens = usage["output_tokens"].as_u64().unwrap_or(0) as usize;
                Event::Nothing
            }
            Some("content_block_start") => {
                let block = &event["content_block"];
                let kind = block["type"].as_str().unwrap_or_default().to_string();
                self.kinds.insert(index, kind.clone());
                match kind.as_str() {
                    "thinking" => {
                        self.thinking_blocks.push(json!({ "type": "thinking", "thinking": block["thinking"].as_str().unwrap_or_default(), "signature": block["signature"].as_str().unwrap_or_default() }));
                        Event::Piece {
                            thinking: block["thinking"]
                                .as_str()
                                .filter(|t| !t.is_empty())
                                .map(str::to_owned),
                            text: None,
                            call: None,
                        }
                    }
                    "redacted_thinking" => {
                        self.thinking_blocks.push(block.clone());
                        Event::Nothing
                    }
                    "tool_use" => Event::Piece {
                        thinking: None,
                        text: None,
                        call: Some(
                            json!({ "tool_calls": [{ "index": index, "id": block["id"], "type": "function", "function": { "name": block["name"], "arguments": "" } }] }),
                        ),
                    },
                    "text" => Event::Piece {
                        thinking: None,
                        text: block["text"]
                            .as_str()
                            .filter(|t| !t.is_empty())
                            .map(str::to_owned),
                        call: None,
                    },
                    _ => Event::Nothing,
                }
            }
            Some("content_block_delta") => {
                let delta = &event["delta"];
                match delta["type"].as_str() {
                    Some("text_delta") => Event::Piece {
                        thinking: None,
                        text: delta["text"].as_str().map(str::to_owned),
                        call: None,
                    },
                    Some("thinking_delta") => {
                        let text = delta["thinking"].as_str().unwrap_or_default();
                        if let Some(block) = self.thinking_blocks.last_mut() {
                            let joined =
                                format!("{}{text}", block["thinking"].as_str().unwrap_or_default());
                            block["thinking"] = json!(joined);
                        }
                        Event::Piece {
                            thinking: Some(text.to_string()).filter(|t| !t.is_empty()),
                            text: None,
                            call: None,
                        }
                    }
                    Some("signature_delta") => {
                        if let Some(block) = self.thinking_blocks.last_mut() {
                            let joined = format!(
                                "{}{}",
                                block["signature"].as_str().unwrap_or_default(),
                                delta["signature"].as_str().unwrap_or_default()
                            );
                            block["signature"] = json!(joined);
                        }
                        Event::Nothing
                    }
                    Some("input_json_delta") => Event::Piece {
                        thinking: None,
                        text: None,
                        call: Some(
                            json!({ "tool_calls": [{ "index": index, "function": { "arguments": delta["partial_json"].as_str().unwrap_or_default() } }] }),
                        ),
                    },
                    _ => Event::Nothing,
                }
            }
            Some("message_delta") => {
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.stop = Some(
                        match reason {
                            "max_tokens" => "length",
                            "tool_use" => "tool_calls",
                            "end_turn" | "stop_sequence" | "pause_turn" => "stop",
                            other => other,
                        }
                        .to_string(),
                    );
                }
                if let Some(output) = event["usage"]["output_tokens"].as_u64() {
                    self.output_tokens = output as usize;
                }
                Event::Nothing
            }
            Some("message_stop") => Event::Done,
            Some("error") => Event::Error(
                event["error"]["message"]
                    .as_str()
                    .unwrap_or("the provider stopped with an error")
                    .to_string(),
            ),
            _ => Event::Nothing,
        }
    }

    /// `(prompt, prompt + answer)` tokens
    pub fn usage(&self) -> Option<(usize, usize)> {
        (self.prompt_tokens + self.output_tokens > 0)
            .then_some((self.prompt_tokens, self.prompt_tokens + self.output_tokens))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::request::{Sampling, Thinking};
    use cluaiz_core::ThinkingBudget;

    fn parts(messages: Vec<Value>, mode: &str, tools: Option<&Value>) -> ChatParts<'static> {
        let sampling: &'static Sampling = Box::leak(Box::new(Sampling {
            temperature: Some(json!(0.2)),
            ..Sampling::default()
        }));
        ChatParts {
            model: "claude-opus-5-5",
            messages,
            sampling,
            thinking: Thinking::of(mode, ThinkingBudget::High),
            tools: tools.map(|t| &*Box::leak(Box::new(t.clone()))),
            response_format: None,
            extra: None,
        }
    }

    #[test]
    fn the_conversation_becomes_messages_with_tool_use_and_results() {
        let messages = vec![
            json!({ "role": "system", "content": "You help." }),
            json!({ "role": "user", "content": "Read a and b" }),
            json!({ "role": "assistant", "content": null, "tool_calls": [
                { "id": "toolu_1", "type": "function", "function": { "name": "read_file", "arguments": "{\"path\":\"a\"}" },
                  THINKING_FIELD: [{ "type": "thinking", "thinking": "Both files.", "signature": "SIG" }] },
                { "id": "call.2", "type": "function", "function": { "name": "read_file", "arguments": "{\"path\":\"b\"}" } },
            ]}),
            json!({ "role": "tool", "tool_call_id": "toolu_1", "content": "A" }),
            json!({ "role": "tool", "tool_call_id": "call.2", "content": "" }),
            json!({ "role": "system", "content": "Call only listed tools." }),
            json!({ "role": "assistant", "content": "A and nothing." }),
        ];
        let (system, out) = convert(messages);
        assert_eq!(system.as_deref(), Some("You help."));
        assert_eq!(out.len(), 4, "{out:#?}");
        // The answer with calls: its thinking block first, unchanged, then the calls
        let calls = &out[1]["content"];
        assert_eq!(
            calls[0],
            json!({ "type": "thinking", "thinking": "Both files.", "signature": "SIG" })
        );
        assert_eq!(calls[1]["type"], "tool_use");
        assert_eq!(calls[1]["input"], json!({ "path": "a" }));
        assert_eq!(calls[2]["id"], "call_2", "ids are made safe");
        // Both results in one user message, before the later note
        let results = &out[2]["content"];
        assert_eq!(
            (
                results[0]["type"].as_str(),
                results[0]["tool_use_id"].as_str()
            ),
            (Some("tool_result"), Some("toolu_1"))
        );
        assert_eq!(
            (
                results[1]["tool_use_id"].as_str(),
                results[1]["content"].as_str()
            ),
            (Some("call_2"), Some("(no output)"))
        );
        assert!(results[2]["text"]
            .as_str()
            .unwrap()
            .starts_with("[System note]"));
        assert_eq!(out[3]["role"], "assistant");
    }

    #[test]
    fn the_request_has_what_claude_takes() {
        let tools = json!([{ "type": "function", "function": { "name": "read_file", "description": "Reads", "parameters": { "type": "object", "properties": { "path": { "type": "string" } } } } }]);
        let body = super::body(
            &Compat::default(),
            parts(
                vec![json!({ "role": "user", "content": "hi" })],
                "on",
                Some(&tools),
            ),
        );
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(
            body["thinking"],
            json!({ "type": "adaptive", "display": "summarized" })
        );
        assert_eq!(body["output_config"]["effort"], "high");
        assert_eq!(
            body["tools"][0],
            json!({ "name": "read_file", "description": "Reads", "input_schema": { "type": "object", "properties": { "path": { "type": "string" } } } })
        );
        assert!(
            body.get("temperature").is_none(),
            "sampling fields are refused by the API version sent"
        );
        // Off: no thinking field, the least effort
        let off = super::body(
            &Compat::default(),
            parts(
                vec![json!({ "role": "user", "content": "hi" })],
                "off",
                None,
            ),
        );
        assert!(off.get("thinking").is_none());
        assert_eq!(off["output_config"]["effort"], "low");
        // Vertex AI: the version in the body, the model in the URL
        let vertex = super::body(
            &Compat {
                vertex: true,
                ..Compat::default()
            },
            parts(
                vec![json!({ "role": "user", "content": "hi" })],
                "auto",
                None,
            ),
        );
        assert_eq!(vertex["anthropic_version"], VERTEX_VERSION);
        assert!(vertex.get("model").is_none());
    }

    #[test]
    fn an_image_is_a_base64_block_and_a_lone_answer_gets_a_user_turn_first() {
        let (_, out) = convert(vec![
            json!({ "role": "assistant", "content": "Hello" }),
            json!({ "role": "user", "content": [{ "type": "text", "text": "This?" }, { "type": "image_url", "image_url": { "url": "data:image/png;base64,iVBOR" } }] }),
        ]);
        assert_eq!(out[0]["role"], "user");
        assert_eq!(
            out[2]["content"][1]["source"],
            json!({ "type": "base64", "media_type": "image/png", "data": "iVBOR" })
        );
    }
}
