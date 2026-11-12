//! Native tool calls: the Engine keeps one form of a call, the `<tool_call>` block with
//! `{"name", "arguments"}` JSON, in its conversations and ledgers. A model that calls tools in its
//! own format gets the conversation as OpenAI messages (its template renders them), and its
//! parsed calls come back in the Engine's form.

use serde_json::{json, Value};

pub const OPEN: &str = "<tool_call>";
pub const CLOSE: &str = "</tool_call>";

/// The inline (Hermes) way of offering tools, for models whose template does not render them:
/// the heading before the `<tools>` block (managed prompt `tools.header`)
pub fn tools_header() -> String {
    format!("{}\n", crate::prompts::text("tools.header"))
}

/// How to call a tool, after the `<tools>` block (managed prompt `tools.call_format`)
pub fn tools_call_format() -> String {
    format!("\n\n{}", crate::prompts::text("tools.call_format"))
}

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
    format!("{}{}{}", tools_header(), tools_block(schemas), tools_call_format())
}

/// The field of a call in the Engine's form that keeps what a provider sent with it besides the
/// name and the arguments: `{"provider", "id", "call": {...}, "function": {...}}`. Gemini 3 sends a
/// `thought_signature` (in `extra_content`) that must come back with the call; other providers
/// want their own call id back. It goes back only to the provider that sent it, and never into
/// an installed model's prompt (`without_provider_data`).
pub const PROVIDER_DATA: &str = "provider_data";

/// Fields of an OpenAI tool call that the Engine's form already holds
const CALL_FIELDS: [&str; 4] = ["index", "id", "type", "function"];
const FUNCTION_FIELDS: [&str; 2] = ["name", "arguments"];

/// A call in the Engine's form
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub name: String,
    /// The arguments as JSON text
    pub arguments: String,
    /// What the provider sent with the call (`PROVIDER_DATA`)
    pub provider_data: Option<Value>,
}

/// What `provider` sent with an OpenAI tool call besides its name and arguments: its id and every
/// other field, kept as sent. None when there is nothing besides them.
pub fn provider_data_of(provider: &str, id: Option<&str>, call_fields: &serde_json::Map<String, Value>, function_fields: &serde_json::Map<String, Value>) -> Option<Value> {
    let call: serde_json::Map<String, Value> = call_fields.iter().filter(|(k, _)| !CALL_FIELDS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
    let function: serde_json::Map<String, Value> = function_fields.iter().filter(|(k, _)| !FUNCTION_FIELDS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
    let id = id.filter(|id| !id.trim().is_empty());
    if id.is_none() && call.is_empty() && function.is_empty() {
        return None;
    }
    let mut data = json!({ "provider": provider });
    if let Some(id) = id {
        data["id"] = json!(id);
    }
    if !call.is_empty() {
        data["call"] = Value::Object(call);
    }
    if !function.is_empty() {
        data["function"] = Value::Object(function);
    }
    Some(data)
}

/// `(role, content)` messages as OpenAI messages: an assistant message's `<tool_call>` blocks
/// become `tool_calls`, and a `tool` message answers the call before it (`tool_call_id`, `name`).
/// What a provider sent with its calls is left out: this form is for installed models.
pub fn openai_messages(messages: &[(String, String)]) -> Vec<Value> {
    openai_messages_for(messages, None)
}

/// As `openai_messages`, for `provider`: its own calls get back their id and the fields it sent
/// with them (`PROVIDER_DATA`); calls another provider or a model made get only name and arguments
pub fn openai_messages_for(messages: &[(String, String)], provider: Option<&str>) -> Vec<Value> {
    let mut out = Vec::with_capacity(messages.len());
    let mut calls = 0usize;
    let mut last_call: Option<(String, String)> = None;
    let mut used_ids: std::collections::HashSet<String> = Default::default();
    // Calls of the last assistant message not answered yet: each `tool` message answers the next one
    let mut waiting: std::collections::VecDeque<(String, String)> = Default::default();
    for (role, content) in messages {
        if role == "assistant" && content.contains(OPEN) {
            let (text, found) = parse_calls(content);
            if !found.is_empty() {
                waiting.clear();
                let tool_calls: Vec<Value> = found
                    .into_iter()
                    .map(|call| {
                        calls += 1;
                        let own = call.provider_data.as_ref().filter(|data| provider.is_some() && data["provider"].as_str() == provider);
                        let id = own
                            .and_then(|data| data["id"].as_str())
                            .filter(|id| !used_ids.contains(*id))
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("call_{calls}"));
                        used_ids.insert(id.clone());
                        last_call = Some((id.clone(), call.name.clone()));
                        waiting.push_back((id.clone(), call.name.clone()));
                        let mut tool_call = json!({ "type": "function", "id": id, "function": { "name": call.name, "arguments": call.arguments } });
                        if let Some(data) = own {
                            for (key, value) in data["call"].as_object().into_iter().flatten() {
                                tool_call[key] = value.clone();
                            }
                            for (key, value) in data["function"].as_object().into_iter().flatten() {
                                tool_call["function"][key] = value.clone();
                            }
                        }
                        tool_call
                    })
                    .collect();
                out.push(json!({ "role": "assistant", "content": text, "tool_calls": tool_calls }));
                continue;
            }
        }
        if role == "tool" {
            let mut message = json!({ "role": "tool", "content": content });
            if let Some((id, name)) = waiting.pop_front().as_ref().or(last_call.as_ref()) {
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

/// What a call without its opening tag starts with
const BARE_CALL: &str = "{\"name\"";

/// A call whose opening tag is missing, `{"name": ...}\n</tool_call>`, as Bonsai models write it:
/// the text before it, the name and the arguments. Only a JSON object with a name counts.
pub fn call_without_open(text: &str) -> Option<(String, String, String)> {
    if text.contains(OPEN) {
        return None;
    }
    let end = text.find(CLOSE)?;
    let start = text[..end].find(BARE_CALL)?;
    let call: Value = serde_json::from_str(text[start..end].trim()).ok()?;
    let name = call.get("name")?.as_str()?.to_string();
    let arguments = match call.get("arguments").or_else(|| call.get("parameters")) {
        Some(Value::String(s)) => s.clone(),
        Some(args) => args.to_string(),
        None => "{}".to_string(),
    };
    Some((text[..start].to_string(), name, arguments))
}

/// How much of a partial answer can go out now: a call without its opening tag may be starting
/// at the rest. The final answer sends what turned out to be plain text.
pub fn before_bare_call(text: &str) -> &str {
    if let Some(start) = text.find(BARE_CALL) {
        return &text[..start];
    }
    let held = (1..BARE_CALL.len()).rev().find(|n| text.ends_with(&BARE_CALL[..*n])).unwrap_or(0);
    &text[..text.len() - held]
}

/// The text around an assistant message's `<tool_call>` blocks, and each call's name and
/// arguments (a JSON string). Blocks that are not a call stay in the text.
pub fn split_calls(content: &str) -> (String, Vec<(String, String)>) {
    let (text, calls) = parse_calls(content);
    (text, calls.into_iter().map(|call| (call.name, call.arguments)).collect())
}

/// As `split_calls`, with what a provider sent with each call
pub fn parse_calls(content: &str) -> (String, Vec<Call>) {
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
            let provider_data = call.get(PROVIDER_DATA).filter(|data| data.is_object()).cloned();
            Some(Call { name, arguments, provider_data })
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
    canonical_call_with(name, arguments, None)
}

/// As `canonical_call`, keeping what the provider sent with the call (`PROVIDER_DATA`)
pub fn canonical_call_with(name: &str, arguments: &str, provider_data: Option<&Value>) -> String {
    let arguments = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!(arguments));
    let mut call = json!({ "name": name, "arguments": arguments });
    if let Some(data) = provider_data.filter(|data| data.is_object()) {
        call[PROVIDER_DATA] = data.clone();
    }
    format!("{OPEN}\n{call}\n{CLOSE}")
}

/// Messages without what providers sent with their calls (`PROVIDER_DATA`), for an installed
/// model's prompt: a chat that moves from a provider's model to an installed one shows the
/// installed model only the calls' names and arguments
pub fn without_provider_data(messages: &[(String, String)]) -> Vec<(String, String)> {
    messages
        .iter()
        .map(|(role, content)| {
            if !content.contains(PROVIDER_DATA) || !content.contains(OPEN) {
                return (role.clone(), content.clone());
            }
            let mut out = String::with_capacity(content.len());
            let mut rest = content.as_str();
            while let Some(start) = rest.find(OPEN) {
                let after = &rest[start + OPEN.len()..];
                let Some(end) = after.find(CLOSE) else { break };
                out.push_str(&rest[..start + OPEN.len()]);
                match serde_json::from_str::<Value>(after[..end].trim()) {
                    Ok(Value::Object(mut call)) if call.contains_key(PROVIDER_DATA) => {
                        call.remove(PROVIDER_DATA);
                        out.push('\n');
                        out.push_str(&Value::Object(call).to_string());
                        out.push('\n');
                    }
                    _ => out.push_str(&after[..end]),
                }
                out.push_str(CLOSE);
                rest = &after[end + CLOSE.len()..];
            }
            out.push_str(rest);
            (role.clone(), out)
        })
        .collect()
}

/// Messages a rendered prompt does not show (indexes into `messages`): a template that drops a
/// role or a field loses content without an error. A message counts as shown when the start of
/// its text (letters and digits only, so escaping and spacing do not matter) is in the prompt;
/// a tool call counts by its function name. Short messages and plain answers are not checked.
pub fn missing_messages(prompt: &str, messages: &[(String, String)]) -> Vec<usize> {
    let squash = |text: &str| -> String { text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect() };
    let shown = squash(prompt);
    messages
        .iter()
        .enumerate()
        .filter(|(_, (role, content))| {
            // Answers are not checked: templates drop their thinking on purpose
            if role == "assistant" && !content.contains(OPEN) {
                return false;
            }
            let probe = if content.contains(OPEN) {
                split_calls(content).1.first().map(|(name, _)| squash(name)).unwrap_or_default()
            } else {
                squash(content).chars().take(24).collect()
            };
            probe.len() >= 6 && !shown.contains(&probe)
        })
        .map(|(i, _)| i)
        .collect()
}

/// Tool results as user turns (`<tool_response>` blocks, the inline form's answer to a call), for a
/// template with no place for the `tool` role. Results in a row become one turn, so roles still
/// alternate.
pub fn tool_results_as_user(messages: &[(String, String)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::with_capacity(messages.len());
    let mut last_was_tool = false;
    for (role, content) in messages {
        if role == "tool" {
            let block = format!("<tool_response>\n{content}\n</tool_response>");
            match out.last_mut() {
                Some((_, previous)) if last_was_tool => *previous = format!("{previous}\n{block}"),
                _ => out.push(("user".to_string(), block)),
            }
            last_was_tool = true;
        } else {
            out.push((role.clone(), content.clone()));
            last_was_tool = false;
        }
    }
    out
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
    fn a_call_without_its_opening_tag_is_still_a_call() {
        let (before, name, args) = call_without_open("Reading.\n{\"name\": \"read_file\", \"arguments\": {\"path\": \"a\"}}\n</tool_call>").unwrap();
        assert_eq!((before.as_str(), name.as_str(), args.as_str()), ("Reading.\n", "read_file", "{\"path\":\"a\"}"));
        // Not a call: no closing tag, not JSON, no name, or a normal block
        assert!(call_without_open("{\"name\": \"x\"}").is_none());
        assert!(call_without_open("{\"name\": oops}\n</tool_call>").is_none());
        assert!(call_without_open("the tag </tool_call> in a sentence").is_none());
        assert!(call_without_open("<tool_call>{\"name\": \"x\"}</tool_call>").is_none());
    }

    #[test]
    fn a_partial_answer_holds_back_what_may_be_a_call() {
        assert_eq!(before_bare_call("Hello there"), "Hello there");
        assert_eq!(before_bare_call("Reading.\n{\"name\": \"re"), "Reading.\n");
        assert_eq!(before_bare_call("Reading.\n{\"na"), "Reading.\n");
        assert_eq!(before_bare_call("Set {"), "Set ");
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
    fn each_result_answers_its_own_call() {
        let two = format!("{}\n{}", canonical_call("read_file", "{\"path\": \"a\"}"), canonical_call("list_dir", "{}"));
        let out = openai_messages(&[msg("user", "look"), msg("assistant", &two), msg("tool", "A"), msg("tool", "B")]);
        assert_eq!(out[1]["tool_calls"][1]["id"], "call_2");
        assert_eq!(out[2]["tool_call_id"], "call_1");
        assert_eq!(out[2]["name"], "read_file");
        assert_eq!(out[3]["tool_call_id"], "call_2");
        assert_eq!(out[3]["name"], "list_dir");
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
    fn what_a_provider_sent_goes_back_only_to_it() {
        let fields = json!({ "index": 0, "id": "g1", "type": "function", "extra_content": { "google": { "thought_signature": "SIG" } } });
        let data = provider_data_of("gemini", Some("g1"), fields.as_object().unwrap(), &Default::default()).unwrap();
        assert_eq!(data, json!({ "provider": "gemini", "id": "g1", "call": { "extra_content": { "google": { "thought_signature": "SIG" } } } }));
        let block = canonical_call_with("read_file", "{\"path\": \"a\"}", Some(&data));
        let (_, calls) = parse_calls(&block);
        assert_eq!(calls[0].provider_data.as_ref(), Some(&data));
        assert_eq!(split_calls(&block).1, vec![("read_file".to_string(), "{\"path\":\"a\"}".to_string())]);
        let conversation = [msg("user", "read a"), msg("assistant", &block), msg("tool", "A")];
        // Its own provider: the id and the signature come back, and the result answers that id
        let out = openai_messages_for(&conversation, Some("gemini"));
        assert_eq!(out[1]["tool_calls"][0]["id"], "g1");
        assert_eq!(out[1]["tool_calls"][0]["extra_content"]["google"]["thought_signature"], "SIG");
        assert_eq!(out[2]["tool_call_id"], "g1");
        // Another provider or an installed model: name and arguments only
        for out in [openai_messages_for(&conversation, Some("groq")), openai_messages(&conversation)] {
            assert_eq!(out[1]["tool_calls"][0]["id"], "call_1");
            assert!(out[1]["tool_calls"][0].get("extra_content").is_none());
        }
        // Nothing beside name and arguments: nothing kept
        assert!(provider_data_of("groq", None, &Default::default(), &Default::default()).is_none());
    }

    #[test]
    fn a_repeated_provider_id_is_not_sent_twice() {
        let data = json!({ "provider": "p", "id": "same" });
        let two = format!("{}\n{}", canonical_call_with("a", "{}", Some(&data)), canonical_call_with("b", "{}", Some(&data)));
        let out = openai_messages_for(&[msg("user", "go"), msg("assistant", &two), msg("tool", "A"), msg("tool", "B")], Some("p"));
        assert_eq!(out[1]["tool_calls"][0]["id"], "same");
        assert_eq!(out[1]["tool_calls"][1]["id"], "call_2");
        assert_eq!(out[3]["tool_call_id"], "call_2");
    }

    #[test]
    fn an_installed_model_never_sees_provider_data() {
        let data = json!({ "provider": "gemini", "call": { "extra_content": { "google": { "thought_signature": "SIG" } } } });
        let content = format!("Reading.\n{}\nand <tool_call>not json</tool_call>", canonical_call_with("read_file", "{\"path\": \"a\"}", Some(&data)));
        let out = without_provider_data(&[msg("user", "hi"), msg("assistant", &content)]);
        assert_eq!(out[0], msg("user", "hi"));
        assert!(!out[1].1.contains("SIG") && !out[1].1.contains(PROVIDER_DATA), "{}", out[1].1);
        assert!(out[1].1.starts_with("Reading.\n<tool_call>"));
        assert!(out[1].1.ends_with("and <tool_call>not json</tool_call>"));
        assert_eq!(split_calls(&out[1].1).1, vec![("read_file".to_string(), "{\"path\":\"a\"}".to_string())]);
    }

    #[test]
    fn dropped_messages_are_found() {
        let messages = vec![
            msg("system", "You are helpful."),
            msg("user", "Read notes.txt please"),
            msg("assistant", "<tool_call>\n{\"name\": \"read_text_file\", \"arguments\": {}}\n</tool_call>"),
            msg("tool", "{\"content\": \"The secret word is MANGO-42.\"}"),
            msg("user", "ok"),
        ];
        let full = "<|im_start|>system\nYou are helpful.<|im_end|>\n<|im_start|>user\nRead notes.txt please<|im_end|>\n<tool_call>{\"name\":\"read_text_file\"}</tool_call>\n<tool_response>{\\\"content\\\": \\\"The secret word is MANGO-42.\\\"}</tool_response>ok";
        assert!(missing_messages(full, &messages).is_empty());
        let no_tool_turns = "system You are helpful. user Read notes.txt please ok";
        assert_eq!(missing_messages(no_tool_turns, &messages), vec![2, 3]);
    }

    #[test]
    fn tool_results_become_one_user_turn() {
        let out = tool_results_as_user(&[
            msg("assistant", "<tool_call>{}</tool_call>"),
            msg("tool", "A"),
            msg("tool", "B"),
            msg("assistant", "done"),
        ]);
        assert_eq!(out.len(), 3);
        assert_eq!(out[1], msg("user", "<tool_response>\nA\n</tool_response>\n<tool_response>\nB\n</tool_response>"));
        assert_eq!(out[2], msg("assistant", "done"));
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
