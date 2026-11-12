//! A provider's streamed answer in the Engine's terms: thinking apart from the answer text, calls
//! put together into `<tool_call>` blocks (with what the provider sent beside them), the tokens
//! it counted and why it stopped. The server's chat and the terminal read every stream here.
//!
//! Providers differ in each part: thinking in `reasoning_content`, `reasoning` or `reasoning_text`,
//! or in the text between `<think>` tags (Qwen, DeepSeek R1) or `<thought>` tags (Gemini); calls
//! numbered and in pieces (OpenAI, Groq) or each sent whole with its own id (Gemini); usage in
//! the last event, in every event (Gemini) or in `x_groq`.

use serde_json::Value;

use super::anthropic::{Event, MessagesStream, THINKING_FIELD};
use super::compat::{Compat, Wire};

/// The thinking tags provider answers are streamed with, in the Engine's form
pub const THINK_START: &str = "<think>";
pub const THINK_END: &str = "</think>";

/// The complete lines of a server-sent event stream, after `chunk` is added to `pending`. A line
/// is decoded only when whole, so a character split between two chunks arrives intact.
pub fn sse_lines(pending: &mut Vec<u8>, chunk: &[u8]) -> Vec<String> {
    pending.extend_from_slice(chunk);
    let mut lines = Vec::new();
    while let Some(end) = pending.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = pending.drain(..=end).collect();
        lines.push(String::from_utf8_lossy(&line).into_owned());
    }
    lines
}

/// The `data:` payload of a stream line; None for comments (`: OPENROUTER PROCESSING`), event
/// names and empty lines
pub fn data_of(line: &str) -> Option<&str> {
    line.trim().strip_prefix("data:").map(str::trim)
}

/// The thinking in a stream delta: providers name it `reasoning_content`, `reasoning` or
/// `reasoning_text`, and some send two with the same text, so only the first is taken
pub fn delta_reasoning(delta: &Value) -> Option<&str> {
    ["reasoning_content", "reasoning", "reasoning_text"]
        .iter()
        .find_map(|field| delta[*field].as_str().filter(|t| !t.is_empty()))
}

/// The tokens a provider counted for a stream event: `(prompt, prompt + answer)`. Groq puts them
/// in `x_groq.usage`.
pub fn event_usage(event: &Value) -> Option<(usize, usize)> {
    let usage = event
        .get("usage")
        .filter(|u| u.is_object())
        .or_else(|| event.pointer("/x_groq/usage"))?;
    let prompt = usage["prompt_tokens"].as_u64().unwrap_or(0) as usize;
    let total = usage["total_tokens"]
        .as_u64()
        .map(|t| t as usize)
        .unwrap_or(prompt + usage["completion_tokens"].as_u64().unwrap_or(0) as usize);
    Some((prompt, total))
}

/// One call being streamed: providers send its name first, its arguments in pieces, and their
/// own fields beside them (Gemini's `extra_content` with its thought signature)
#[derive(Default)]
struct StreamedCall {
    name: String,
    arguments: String,
    id: Option<String>,
    call_fields: serde_json::Map<String, Value>,
    function_fields: serde_json::Map<String, Value>,
}

/// The calls of a streamed answer, put together from the `tool_calls` of each delta
#[derive(Default)]
pub struct StreamedCalls {
    calls: std::collections::BTreeMap<u64, StreamedCall>,
}

impl StreamedCalls {
    /// Adds a delta's `tool_calls`; the name and the arguments so far of the last call touched
    pub fn add(&mut self, delta: &Value) -> Option<(String, String)> {
        let mut touched = None;
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            let slot = self.slot_of(call);
            let entry = self.calls.entry(slot).or_default();
            if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                entry.name.push_str(name);
            }
            match call.pointer("/function/arguments") {
                Some(Value::String(arguments)) => entry.arguments.push_str(arguments),
                Some(arguments) if arguments.is_object() => {
                    entry.arguments.push_str(&arguments.to_string())
                }
                _ => {}
            }
            if let Some(id) = call["id"].as_str().filter(|id| !id.is_empty()) {
                entry.id = Some(id.to_owned());
            }
            for (key, value) in call.as_object().into_iter().flatten() {
                if !value.is_null() {
                    entry.call_fields.insert(key.clone(), value.clone());
                }
            }
            for (key, value) in call["function"].as_object().into_iter().flatten() {
                if !value.is_null() {
                    entry.function_fields.insert(key.clone(), value.clone());
                }
            }
            touched = Some((entry.name.clone(), entry.arguments.clone()));
        }
        touched
    }

    /// Which call a streamed piece belongs to. OpenAI numbers the calls (`index`) and sends the
    /// id once; Gemini sends each parallel call whole, with its own id and no index (or the same
    /// index); a piece with neither continues the last call.
    fn slot_of(&self, call: &Value) -> u64 {
        let id = call["id"].as_str().filter(|id| !id.is_empty());
        let next = self.calls.keys().next_back().map_or(0, |last| last + 1);
        if let Some(id) = id {
            if let Some((slot, _)) = self.calls.iter().find(|(_, c)| c.id.as_deref() == Some(id)) {
                return *slot;
            }
        }
        match call["index"].as_u64() {
            // The same index under another id is another call
            Some(index) => match self.calls.get(&index) {
                Some(taken) if id.is_some() && taken.id.is_some() && taken.id.as_deref() != id => {
                    next
                }
                _ => index,
            },
            None => {
                let starts = id.is_some()
                    || call
                        .pointer("/function/name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| !name.is_empty());
                if starts || self.calls.is_empty() {
                    next
                } else {
                    next - 1
                }
            }
        }
    }

    /// Puts a field on the first call: the thinking blocks of an Anthropic answer go back with it
    pub fn attach_to_first(&mut self, key: &str, value: Value) {
        if let Some(first) = self.calls.values_mut().find(|c| !c.name.is_empty()) {
            first.call_fields.insert(key.to_string(), value);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.calls.values().all(|c| c.name.is_empty())
    }

    /// Each named call in the Engine's form (`<tool_call>` blocks), with what `provider` sent
    /// with it
    pub fn blocks(self, provider: &str) -> Vec<String> {
        self.calls
            .into_values()
            .filter(|call| !call.name.is_empty())
            .map(|call| {
                let arguments = if call.arguments.trim().is_empty() {
                    "{}"
                } else {
                    call.arguments.as_str()
                };
                let data = cluaiz_core::tool_calls::provider_data_of(
                    provider,
                    call.id.as_deref(),
                    &call.call_fields,
                    &call.function_fields,
                );
                cluaiz_core::tool_calls::canonical_call_with(&call.name, arguments, data.as_ref())
            })
            .collect()
    }
}

/// Gemini's thinking taken out of the answer text. Google AI Studio sends it between `<thought>`
/// tags (the closing tag may come in a later piece, with the answer); Vertex AI sends it without
/// tags, in pieces marked `extra_content.google.thought`. A tag cut between two pieces is held
/// back until it is whole.
#[derive(Default)]
struct ThoughtSplit {
    held: String,
    inside: bool,
}

impl ThoughtSplit {
    const OPEN: &'static str = "<thought>";
    const CLOSE: &'static str = "</thought>";

    /// `(thinking, answer)` of the next piece of text
    fn push(&mut self, text: &str) -> (String, String) {
        self.held.push_str(text);
        let mut rest = std::mem::take(&mut self.held);
        let (mut thinking, mut answer) = (String::new(), String::new());
        loop {
            let tag = if self.inside { Self::CLOSE } else { Self::OPEN };
            match rest.find(tag) {
                Some(at) => {
                    if self.inside {
                        thinking.push_str(&rest[..at])
                    } else {
                        answer.push_str(&rest[..at])
                    }
                    rest = rest[at + tag.len()..].to_string();
                    self.inside = !self.inside;
                }
                None => {
                    // The end may be the start of the tag: keep it for the next piece
                    let cut = (1..tag.len())
                        .rev()
                        .find(|n| rest.ends_with(&tag[..*n]))
                        .unwrap_or(0);
                    self.held = rest[rest.len() - cut..].to_string();
                    let whole = &rest[..rest.len() - cut];
                    if self.inside {
                        thinking.push_str(whole)
                    } else {
                        answer.push_str(whole)
                    }
                    return (thinking, answer);
                }
            }
        }
    }

    /// What was held back at the end, as answer text
    fn flush(&mut self) -> String {
        if self.inside {
            self.held.clear()
        }
        std::mem::take(&mut self.held)
    }
}

/// What one stream event said
#[derive(Debug, PartialEq)]
pub enum Read {
    /// Nothing to pass on (a role, an empty delta, a comment)
    Nothing,
    /// Thinking and answer text, and the call being streamed (name, arguments so far)
    Piece {
        thinking: Option<String>,
        text: Option<String>,
        call: Option<(String, String)>,
    },
    /// `[DONE]`
    Done,
    /// The provider stopped with an error; `raw` is the whole event
    Error { message: String, raw: String },
}

/// Reads a provider's stream, one `data:` payload at a time
pub struct Decoder {
    tags: Option<ThoughtSplit>,
    /// Set for an Anthropic Messages stream
    messages: Option<MessagesStream>,
    calls: StreamedCalls,
    usage: Option<(usize, usize)>,
    finish: Option<String>,
    produced: bool,
}

/// What a whole stream gave besides its pieces
pub struct Finished {
    /// Answer text held back at the end (an unfinished tag)
    pub text: String,
    /// The calls in the Engine's form
    pub calls: Vec<String>,
    /// `(prompt, prompt + answer)` tokens, as the provider counted them
    pub usage: Option<(usize, usize)>,
    /// The provider's `finish_reason`
    pub finish: Option<String>,
}

impl Decoder {
    pub fn new(compat: &Compat) -> Self {
        Self {
            tags: compat.thought_tags().then(ThoughtSplit::default),
            messages: (compat.wire == Wire::Anthropic).then(MessagesStream::default),
            calls: StreamedCalls::default(),
            usage: None,
            finish: None,
            produced: false,
        }
    }

    pub fn read(&mut self, data: &str) -> Read {
        if data == "[DONE]" {
            return Read::Done;
        }
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return Read::Nothing;
        };
        if let Some(messages) = &mut self.messages {
            return match messages.read(&event) {
                Event::Nothing => Read::Nothing,
                Event::Done => Read::Done,
                Event::Error(message) => Read::Error {
                    message,
                    raw: data.to_string(),
                },
                Event::Piece {
                    thinking,
                    text,
                    call,
                } => {
                    let call = call.and_then(|delta| self.calls.add(&delta));
                    if thinking.is_none() && text.is_none() && call.is_none() {
                        return Read::Nothing;
                    }
                    self.produced = true;
                    Read::Piece {
                        thinking,
                        text,
                        call,
                    }
                }
            };
        }
        let error = event
            .pointer("/error/message")
            .and_then(Value::as_str)
            .or_else(|| event["error"].as_str());
        if let Some(message) = error {
            return Read::Error {
                message: message.to_string(),
                raw: data.to_string(),
            };
        }
        if let Some(counted) = event_usage(&event) {
            self.usage = Some(counted);
        }
        let Some(choice) = event["choices"].get(0) else {
            return Read::Nothing;
        };
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.finish = Some(reason.to_string());
        }
        let delta = &choice["delta"];
        let mut thinking = delta_reasoning(delta).map(str::to_owned);
        // A piece Gemini marks as thought is thinking, tags or not
        let marked = delta
            .pointer("/extra_content/google/thought")
            .and_then(Value::as_bool)
            == Some(true);
        let mut text = None;
        if let Some(content) = delta["content"].as_str().filter(|t| !t.is_empty()) {
            let (thought, answer) = match &mut self.tags {
                Some(split) => split.push(content),
                None => (String::new(), content.to_string()),
            };
            let thought = if marked { thought + &answer } else { thought };
            let answer = if marked { String::new() } else { answer };
            if !thought.is_empty() {
                thinking = Some(thinking.unwrap_or_default() + &thought);
            }
            text = Some(answer).filter(|t| !t.is_empty());
        }
        let call = self.calls.add(delta);
        if thinking.is_none() && text.is_none() && call.is_none() {
            return Read::Nothing;
        }
        self.produced = true;
        Read::Piece {
            thinking,
            text,
            call,
        }
    }

    /// Whether the stream gave any thinking, text or call so far
    pub fn produced(&self) -> bool {
        self.produced
    }

    pub fn finish(mut self, provider: &str) -> Finished {
        let text = self
            .tags
            .as_mut()
            .map(ThoughtSplit::flush)
            .unwrap_or_default();
        if let Some(messages) = self.messages.take() {
            // The thinking of an answer with calls goes back with its first call, unchanged
            if !messages.thinking_blocks.is_empty() {
                self.calls.attach_to_first(
                    THINKING_FIELD,
                    Value::Array(messages.thinking_blocks.clone()),
                );
            }
            self.usage = messages.usage();
            self.finish = messages.stop.clone();
        }
        Finished {
            text,
            calls: self.calls.blocks(provider),
            usage: self.usage,
            finish: self.finish,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_character_split_between_chunks_arrives_whole() {
        let line = "data: {\"t\":\"नमस्ते 👋\"}\n".as_bytes();
        let cut = line.iter().position(|&b| b >= 0x80).unwrap() + 1;
        let mut pending = Vec::new();
        assert!(sse_lines(&mut pending, &line[..cut]).is_empty());
        assert_eq!(
            sse_lines(&mut pending, &line[cut..]),
            vec!["data: {\"t\":\"नमस्ते 👋\"}\n".to_string()]
        );
        assert!(pending.is_empty());
        assert_eq!(
            delta_reasoning(&json!({ "reasoning_content": "a", "reasoning": "a" })),
            Some("a")
        );
        assert_eq!(
            delta_reasoning(&json!({ "reasoning_content": "", "reasoning": "b" })),
            Some("b")
        );
    }

    fn names(calls: StreamedCalls) -> Vec<(String, String)> {
        calls
            .blocks("p")
            .iter()
            .flat_map(|block| cluaiz_core::tool_calls::split_calls(block).1)
            .collect()
    }

    #[test]
    fn streamed_calls_are_put_together_as_each_provider_sends_them() {
        // OpenAI, Groq: numbered calls, the id and name once, the arguments in pieces
        let mut openai = StreamedCalls::default();
        for delta in [
            json!({ "tool_calls": [{ "index": 0, "id": "call_a", "type": "function", "function": { "name": "read_file", "arguments": "" } }] }),
            json!({ "tool_calls": [{ "index": 0, "function": { "arguments": "{\"path\":" } }] }),
            json!({ "tool_calls": [{ "index": 0, "function": { "arguments": "\"a.txt\"}" } }] }),
            json!({ "tool_calls": [{ "index": 1, "id": "call_b", "type": "function", "function": { "name": "list_dir", "arguments": "{}" } }] }),
        ] {
            openai.add(&delta);
        }
        assert_eq!(
            names(openai),
            vec![
                ("read_file".to_string(), "{\"path\":\"a.txt\"}".to_string()),
                ("list_dir".to_string(), "{}".to_string())
            ]
        );

        // Gemini: each parallel call whole, in its own event, without an index; only the first
        // carries the thought signature
        let mut gemini = StreamedCalls::default();
        gemini.add(&json!({ "tool_calls": [{ "id": "g1", "type": "function", "extra_content": { "google": { "thought_signature": "SIG" } }, "function": { "name": "read_item", "arguments": "{\"name\":\"math\"}" } }] }));
        gemini.add(&json!({ "tool_calls": [{ "id": "g2", "type": "function", "function": { "name": "load_tool", "arguments": "{\"id\":\"math\"}" } }] }));
        let blocks = gemini.blocks("gemini");
        assert_eq!(blocks.len(), 2, "two calls stay two");
        let (_, first) = cluaiz_core::tool_calls::parse_calls(&blocks[0]);
        assert_eq!(first[0].name, "read_item");
        assert_eq!(
            first[0].provider_data.as_ref().unwrap()["call"]["extra_content"]["google"]
                ["thought_signature"],
            "SIG"
        );
        assert_eq!(
            cluaiz_core::tool_calls::parse_calls(&blocks[1]).1[0].name,
            "load_tool"
        );

        // The same index under two ids is two calls; pieces with neither go to the last call
        let mut mixed = StreamedCalls::default();
        mixed.add(&json!({ "tool_calls": [{ "index": 0, "id": "x1", "function": { "name": "a", "arguments": "{}" } }] }));
        mixed.add(
            &json!({ "tool_calls": [{ "index": 0, "id": "x2", "function": { "name": "b" } }] }),
        );
        mixed.add(&json!({ "tool_calls": [{ "function": { "arguments": "{\"k\":1}" } }] }));
        assert_eq!(
            names(mixed),
            vec![
                ("a".to_string(), "{}".to_string()),
                ("b".to_string(), "{\"k\":1}".to_string())
            ]
        );

        // A call without arguments gets an empty object; nothing beside name and arguments: no provider data
        let mut bare = StreamedCalls::default();
        bare.add(&json!({ "tool_calls": [{ "function": { "name": "now" } }] }));
        let block = bare.blocks("p").remove(0);
        assert!(!block.contains(cluaiz_core::tool_calls::PROVIDER_DATA));
        assert_eq!(
            cluaiz_core::tool_calls::split_calls(&block).1,
            vec![("now".to_string(), "{}".to_string())]
        );
        assert_eq!(
            event_usage(
                &json!({ "x_groq": { "usage": { "prompt_tokens": 5, "completion_tokens": 2 } } })
            ),
            Some((5, 7))
        );
    }

    #[test]
    fn gemini_thinking_is_taken_out_of_the_answer_with_or_without_tags() {
        // AI Studio: tags, cut anywhere, the closing one with the answer
        let mut split = ThoughtSplit::default();
        let (mut thinking, mut answer) = (String::new(), String::new());
        for piece in ["<thou", "ght>a plan</tho", "ught>The answer <", "b>"] {
            let (t, a) = split.push(piece);
            thinking += &t;
            answer += &a;
        }
        answer += &split.flush();
        assert_eq!(
            (thinking.as_str(), answer.as_str()),
            ("a plan", "The answer <b>")
        );

        // Vertex AI: no tags, the thinking pieces marked; the decoder reads both as thinking
        let compat = Compat {
            thinking: super::super::compat::ThinkingFormat::Gemini,
            ..Compat::default()
        };
        let mut decoder = Decoder::new(&compat);
        let marked = decoder.read(r#"{"choices":[{"delta":{"content":"**Plan** check 17","extra_content":{"google":{"thought":true}}}}]}"#);
        assert_eq!(
            marked,
            Read::Piece {
                thinking: Some("**Plan** check 17".into()),
                text: None,
                call: None
            }
        );
        let answer = decoder.read(r#"{"choices":[{"delta":{"content":"No, 391 is 17 x 23."}}]}"#);
        assert_eq!(
            answer,
            Read::Piece {
                thinking: None,
                text: Some("No, 391 is 17 x 23.".into()),
                call: None
            }
        );
        // AI Studio through the decoder: the marked first piece opens the tag, the next closes it
        let mut decoder = Decoder::new(&compat);
        assert_eq!(decoder.read(r#"{"choices":[{"delta":{"content":"<thought>**Plan**","extra_content":{"google":{"thought":true}}}}]}"#), Read::Piece { thinking: Some("**Plan**".into()), text: None, call: None });
        assert_eq!(
            decoder.read(r#"{"choices":[{"delta":{"content":"</thought>No."}}]}"#),
            Read::Piece {
                thinking: None,
                text: Some("No.".into()),
                call: None
            }
        );
    }
}
