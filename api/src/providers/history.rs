//! The Engine's conversation as a provider's messages. Calls in the Engine's `<tool_call>` form
//! become OpenAI `tool_calls`; the provider gets back the ids and fields it sent with its own calls
//! (Gemini's thought signature), and only names and arguments of calls another model made. Then
//! the provider's own rules (`compat.rs`): the form of its call ids, `reasoning_content` on
//! assistant messages, no null `content`.

use cluaiz_core::providers::Target;
use serde_json::{json, Value};

use super::compat::{Compat, IdForm};

/// A conversation in the Engine's form (`<tool_call>` blocks in assistant text, `tool` results)
/// in the OpenAI form: calls become `tool_calls`, each result answers its call by `tool_call_id`,
/// a call without a result is answered "(not run)", and a result without a call is user text.
/// For `to`, the provider's own call data comes back and its rules are applied.
pub fn openai_conversation(pairs: &[(String, String)], to: Option<&Target>) -> Vec<Value> {
    let converted =
        cluaiz_core::tool_calls::openai_messages_for(pairs, to.map(|t| t.provider.as_str()));
    let mut out: Vec<Value> = Vec::with_capacity(converted.len());
    let mut waiting: Vec<String> = Vec::new();
    let close = |out: &mut Vec<Value>, waiting: &mut Vec<String>| {
        for id in waiting.drain(..) {
            out.push(json!({ "role": "tool", "tool_call_id": id, "content": "(not run)" }));
        }
    };
    for mut message in converted {
        if message["role"] == "tool" {
            let id = message["tool_call_id"].as_str().map(str::to_owned);
            match id.and_then(|id| waiting.iter().position(|w| *w == id)) {
                Some(at) => {
                    waiting.remove(at);
                    out.push(message);
                }
                None => out.push(json!({ "role": "user", "content": format!("Tool result:\n{}", message["content"].as_str().unwrap_or_default()) })),
            }
            continue;
        }
        close(&mut out, &mut waiting);
        if let Some(calls) = message["tool_calls"].as_array() {
            waiting = calls
                .iter()
                .filter_map(|c| c["id"].as_str().map(str::to_owned))
                .collect();
            if message["content"].as_str().is_some_and(str::is_empty) {
                message["content"] = Value::Null;
            }
        }
        out.push(message);
    }
    close(&mut out, &mut waiting);
    if let Some(target) = to {
        fit(&mut out, &Compat::of(target));
    }
    out
}

/// A conversation with attachments in the OpenAI form: as `openai_conversation`, and a message
/// with images or sounds gets content parts (images as PNG `image_url`, sounds as WAV
/// `input_audio`). Files arrive as their text already.
pub fn openai_conversation_with_parts(
    messages: Vec<cluaiz_core::ChatMessage<Vec<cluaiz_core::media::PreparedPart>>>,
    to: Option<&Target>,
) -> Result<Vec<Value>, String> {
    use base64::Engine as _;
    use cluaiz_core::media::PreparedPart;
    let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    // A message with attachments is marked in its text, converted with the others, then given
    // its content parts
    let mut attached: Vec<Vec<Value>> = Vec::new();
    let mut pairs = Vec::with_capacity(messages.len());
    for message in messages {
        let mut text = String::new();
        let mut parts = Vec::new();
        for part in message.content {
            match part {
                PreparedPart::Text { text: part } => text.push_str(&part),
                PreparedPart::Image { image } => {
                    let png = cluaiz_media::image::encode_png(&image)
                        .map_err(|e| format!("an attached image could not be prepared: {e}"))?;
                    parts.push(json!({ "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{}", encode(&png)) } }));
                }
                PreparedPart::Audio { audio } => {
                    let channels = audio.channels().max(1) as usize;
                    let mono: Vec<f32> = audio
                        .samples()
                        .chunks(channels)
                        .map(|f| f.iter().sum::<f32>() / f.len() as f32)
                        .collect();
                    let wav = cluaiz_media::wav::encode_pcm16(
                        &mono,
                        audio.sample_rate(),
                        Default::default(),
                    )
                    .map_err(|e| format!("an attached sound could not be prepared: {e}"))?;
                    parts.push(json!({ "type": "input_audio", "input_audio": { "data": encode(&wav), "format": "wav" } }));
                }
            }
        }
        if !parts.is_empty() {
            text = format!("{MEDIA_MARK}{}\u{1}{text}", attached.len());
            attached.push(parts);
        }
        pairs.push((message.role, text));
    }
    let mut out = openai_conversation(&pairs, to);
    for message in &mut out {
        let Some(text) = message["content"]
            .as_str()
            .and_then(|t| t.strip_prefix(MEDIA_MARK))
            .map(str::to_owned)
        else {
            continue;
        };
        let Some((index, rest)) = text.split_once('\u{1}') else {
            continue;
        };
        let Some(parts) = index.parse::<usize>().ok().and_then(|i| attached.get(i)) else {
            continue;
        };
        let mut content = vec![json!({ "type": "text", "text": rest })];
        content.extend(parts.iter().cloned());
        message["content"] = json!(content);
    }
    Ok(out)
}

/// Marks a message whose attachments are put back after the conversion
const MEDIA_MARK: &str = "\u{1}cluaiz-attached-";

/// The provider's own rules on a converted conversation
fn fit(messages: &mut [Value], compat: &Compat) {
    for message in messages.iter_mut() {
        let calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .is_some_and(|c| !c.is_empty());
        if calls && compat.content_not_null && message["content"].is_null() {
            message["content"] = json!("");
        }
        if calls && compat.reasoning_content_on_calls && message.get("reasoning_content").is_none()
        {
            message["reasoning_content"] = json!("");
        }
        if compat.tool_call_ids == IdForm::Alnum9 {
            if let Some(calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) {
                for call in calls {
                    if let Some(id) = call["id"].as_str() {
                        call["id"] = json!(alnum9(id));
                    }
                }
            }
            if let Some(id) = message.get("tool_call_id").and_then(Value::as_str) {
                message["tool_call_id"] = json!(alnum9(id));
            }
        }
    }
}

/// An id as nine letters and digits: kept when it is one, else the same nine for the same id
fn alnum9(id: &str) -> String {
    if id.len() == 9 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return id.to_string();
    }
    // FNV-1a: the same id gives the same nine characters in every request
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    (0..9)
        .map(|_| {
            let c = DIGITS[(hash % 62) as usize] as char;
            hash /= 62;
            c
        })
        .collect()
}

/// Takes out the oldest turn (a user message and every message after it up to the next user
/// message); system messages at the start stay. Returns how many messages went: 0 when only the
/// last turn is left, which is never taken out.
pub fn drop_oldest_turn(messages: &mut Vec<Value>) -> usize {
    let is_user = |m: &Value| m.get("role").and_then(Value::as_str) == Some("user");
    let Some(first) = messages.iter().position(is_user) else {
        return 0;
    };
    let Some(next) = messages
        .iter()
        .skip(first + 1)
        .position(is_user)
        .map(|at| at + first + 1)
    else {
        return 0;
    };
    messages.drain(first..next);
    next - first
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_core::providers::Provider;
    use cluaiz_core::tool_calls::{canonical_call, canonical_call_with};

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(r, c)| (r.to_string(), c.to_string()))
            .collect()
    }

    fn target(name: &str, url: &str) -> Target {
        Target {
            provider: name.into(),
            config: Provider::new(url),
            model: "m".into(),
        }
    }

    #[test]
    fn each_provider_gets_the_conversation_in_its_own_rules() {
        let gemini_call = canonical_call_with(
            "read_file",
            "{\"path\":\"a\"}",
            Some(
                &json!({ "provider": "gemini", "id": "g-1", "call": { "extra_content": { "google": { "thought_signature": "SIG" } } } }),
            ),
        );
        let conversation = pairs(&[
            ("user", "read a"),
            ("assistant", &gemini_call),
            ("tool", "A"),
            ("assistant", "It says A."),
        ]);

        // Gemini: its id and its signature come back
        let out = openai_conversation(
            &conversation,
            Some(&target(
                "gemini",
                "https://generativelanguage.googleapis.com/v1beta/openai",
            )),
        );
        assert_eq!(
            out[1]["tool_calls"][0]["extra_content"]["google"]["thought_signature"],
            "SIG"
        );
        assert_eq!(
            (
                out[1]["tool_calls"][0]["id"].as_str(),
                out[2]["tool_call_id"].as_str()
            ),
            (Some("g-1"), Some("g-1"))
        );
        assert_eq!(out[1]["content"], Value::Null);

        // DeepSeek: names and arguments only, `reasoning_content` on the call message
        let out = openai_conversation(
            &conversation,
            Some(&target("deepseek", "https://api.deepseek.com/v1")),
        );
        assert!(out[1]["tool_calls"][0].get("extra_content").is_none());
        assert_eq!(out[1]["reasoning_content"], "");
        assert!(
            out[3].get("reasoning_content").is_none(),
            "only on messages with calls"
        );

        // Mistral: nine-character ids, the same in the call and its result
        let out = openai_conversation(
            &conversation,
            Some(&target("mistral", "https://api.mistral.ai/v1")),
        );
        let id = out[1]["tool_calls"][0]["id"].as_str().unwrap().to_string();
        assert!(
            id.len() == 9 && id.chars().all(|c| c.is_ascii_alphanumeric()),
            "{id}"
        );
        assert_eq!(out[2]["tool_call_id"], json!(id));

        // A provider that wants "" instead of null
        let mut strict = target("strict", "http://127.0.0.1:9/v1");
        strict.config.compat.content_not_null = Some(true);
        assert_eq!(
            openai_conversation(&conversation, Some(&strict))[1]["content"],
            ""
        );

        // No provider (an installed model's template): nothing of the provider's
        let out = openai_conversation(
            &pairs(&[
                ("user", "go"),
                ("assistant", &canonical_call("x", "{}")),
                ("tool", "r"),
            ]),
            None,
        );
        assert_eq!(out[1]["tool_calls"][0]["id"], "call_1");
    }

    #[test]
    fn nine_character_ids_are_stable_and_distinct() {
        assert_eq!(alnum9("call_1"), alnum9("call_1"));
        assert_ne!(alnum9("call_1"), alnum9("call_2"));
        assert_eq!(alnum9("D681PevKs"), "D681PevKs");
    }

    #[test]
    fn the_oldest_turn_goes_and_the_system_prompt_and_last_turn_stay() {
        let mut messages = vec![
            json!({ "role": "system", "content": "s" }),
            json!({ "role": "user", "content": "1" }),
            json!({ "role": "assistant", "tool_calls": [] }),
            json!({ "role": "tool", "content": "r" }),
            json!({ "role": "assistant", "content": "a1" }),
            json!({ "role": "user", "content": "2" }),
        ];
        assert_eq!(drop_oldest_turn(&mut messages), 4);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["content"], "2");
        assert_eq!(drop_oldest_turn(&mut messages), 0);
    }
}
