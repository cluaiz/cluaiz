//! Native tool calls: the Engine keeps one form of a call, the `<tool_call>` block with
//! `{"name", "arguments"}` JSON, in its conversations and ledgers. A model that calls tools in its
//! own format gets the conversation as OpenAI messages (its template renders them), and its
//! parsed calls come back in the Engine's form.

use serde_json::{json, Value};

const OPEN: &str = "<tool_call>";
const CLOSE: &str = "</tool_call>";

/// The inline (Hermes) way of offering tools, for models whose template does not render them
pub const TOOLS_HEADER: &str = "# Available Tools\nYou have access to the following tools:\n";
pub const TOOLS_CALL_FORMAT: &str = "\n\nWhen you need to call a tool, you MUST output a <tool_call> XML block with a JSON object containing \"name\" and \"arguments\":\n<tool_call>\n{\"name\": \"tool_name\", \"arguments\": {\"arg_name\": \"value\"}}\n</tool_call>\nDo not output conversational filler text before or after the <tool_call> block.";

/// The `<tools>` block: one function schema per line
pub fn tools_block(schemas: &[Value]) -> String {
    if schemas.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = schemas.iter().map(Value::to_string).collect();
    format!("<tools>\n{}\n</tools>", lines.join("\n"))
}

/// Tools offered inline: header, `<tools>` block and how to call them
pub fn inline_tools(schemas: &[Value]) -> String {
    format!("{TOOLS_HEADER}{}{TOOLS_CALL_FORMAT}", tools_block(schemas))
}

/// `(role, content)` messages as OpenAI messages: an assistant message's `<tool_call>` blocks
/// become `tool_calls`, and a `tool` message answers the call before it (`tool_call_id`, `name`)
pub fn openai_messages(messages: &[(String, String)]) -> Vec<Value> {
    let mut out = Vec::with_capacity(messages.len());
    let mut calls = 0usize;
    let mut last_call: Option<(String, String)> = None;
    for (role, content) in messages {
        if role == "assistant" && content.contains(OPEN) {
            let (text, found) = split_calls(content);
            if !found.is_empty() {
                let tool_calls: Vec<Value> = found
                    .into_iter()
                    .map(|(name, arguments)| {
                        calls += 1;
                        let id = format!("call_{calls}");
                        last_call = Some((id.clone(), name.clone()));
                        json!({ "type": "function", "id": id, "function": { "name": name, "arguments": arguments } })
                    })
                    .collect();
                out.push(json!({ "role": "assistant", "content": text, "tool_calls": tool_calls }));
                continue;
            }
        }
        if role == "tool" {
            let mut message = json!({ "role": "tool", "content": content });
            if let Some((id, name)) = &last_call {
                message["tool_call_id"] = json!(id);
                message["name"] = json!(name);
            }
            out.push(message);
            continue;
        }
        out.push(json!({ "role": role, "content": content }));
    }
    out
}

/// The text around an assistant message's `<tool_call>` blocks, and each call's name and
/// arguments (a JSON string). Blocks that are not a call stay in the text.
fn split_calls(content: &str) -> (String, Vec<(String, String)>) {
    let mut text = String::new();
    let mut calls = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find(CLOSE) else { break };
        let call = serde_json::from_str::<Value>(after[..end].trim()).ok().and_then(|call| {
            let name = call.get("name")?.as_str()?.to_string();
            let arguments = match call.get("arguments").or_else(|| call.get("parameters")) {
                Some(Value::String(s)) => s.clone(),
                Some(args) => args.to_string(),
                None => "{}".to_string(),
            };
            Some((name, arguments))
        });
        match call {
            Some(call) => {
                text.push_str(&rest[..start]);
                calls.push(call);
            }
            None => text.push_str(&rest[..start + OPEN.len() + end + CLOSE.len()]),
        }
        rest = &after[end + CLOSE.len()..];
    }
    text.push_str(rest);
    (text.trim().to_string(), calls)
}

/// A parsed call in the Engine's form. `arguments` is the model's JSON text (kept as a string
/// when it is not valid JSON, so the tool reports the error).
pub fn canonical_call(name: &str, arguments: &str) -> String {
    let arguments = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!(arguments));
    format!("{OPEN}\n{}\n{CLOSE}", json!({ "name": name, "arguments": arguments }))
}

/// The visible answer while a parser reads the output again after every token: only text that
/// extends what was already sent goes out (a parser may hold back or revise its last bit)
#[derive(Default)]
pub struct ContentStream {
    sent: String,
}

impl ContentStream {
    /// The new part of `content`, if it extends what was sent
    pub fn delta(&mut self, content: &str) -> Option<String> {
        if content.len() > self.sent.len() && content.starts_with(&self.sent) {
            let delta = content[self.sent.len()..].to_string();
            self.sent = content.to_string();
            Some(delta)
        } else {
            None
        }
    }

    pub fn sent(&self) -> &str {
        &self.sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> (String, String) {
        (role.to_string(), content.to_string())
    }

    #[test]
    fn calls_become_openai_tool_calls() {
        let out = openai_messages(&[
            msg("user", "read notes.txt"),
            msg("assistant", "<tool_call>\n{\"name\": \"read_file\", \"arguments\": {\"path\": \"notes.txt\"}}\n</tool_call>"),
            msg("tool", "MANGO-42"),
            msg("assistant", "The secret is MANGO-42."),
        ]);
        assert_eq!(out[1]["content"], "");
        assert_eq!(out[1]["tool_calls"][0]["function"]["name"], "read_file");
        assert_eq!(out[1]["tool_calls"][0]["function"]["arguments"], "{\"path\":\"notes.txt\"}");
        assert_eq!(out[2]["tool_call_id"], "call_1");
        assert_eq!(out[2]["name"], "read_file");
        assert!(out[3].get("tool_calls").is_none());
    }

    #[test]
    fn text_around_a_call_is_kept_and_broken_blocks_stay_text() {
        let (text, calls) = split_calls("Let me check.\n<tool_call>{\"name\": \"x\", \"arguments\": \"{}\"}</tool_call>");
        assert_eq!(text, "Let me check.");
        assert_eq!(calls, vec![("x".to_string(), "{}".to_string())]);
        let (text, calls) = split_calls("<tool_call>not json</tool_call> after");
        assert_eq!(text, "<tool_call>not json</tool_call> after");
        assert!(calls.is_empty());
    }

    #[test]
    fn canonical_calls_round_trip() {
        let call = canonical_call("add", "{\"a\": 17, \"b\": 25}");
        let (_, calls) = split_calls(&call);
        assert_eq!(calls, vec![("add".to_string(), "{\"a\":17,\"b\":25}".to_string())]);
        assert!(canonical_call("add", "{broken").contains("\"arguments\":\"{broken\""));
    }

    #[test]
    fn only_growing_content_is_sent() {
        let mut stream = ContentStream::default();
        assert_eq!(stream.delta("Hel").as_deref(), Some("Hel"));
        assert_eq!(stream.delta("Hel"), None);
        assert_eq!(stream.delta("Hello").as_deref(), Some("lo"));
        assert_eq!(stream.delta("Help"), None);
        assert_eq!(stream.sent(), "Hello");
    }
}
