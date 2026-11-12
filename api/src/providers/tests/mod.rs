//! Conformance: each provider's stream, read as the server's chat reads it, gives the same Engine
//! result: the thinking apart from the answer, every call whole with its arguments and the data
//! its provider wants back, the tokens counted, and why it stopped. The streams in `fixtures/` are
//! recorded from the provider (Gemini, Groq) or written from its documented format (each file
//! says which on its first line); a new provider gets a file and a case here.

use serde_json::Value;

use super::compat::{detect, Compat};
use super::stream::{data_of, Decoder, Read, THINK_END, THINK_START};
use cluaiz_core::tool_calls::{parse_calls, Call};

/// What the Engine gets from a whole stream
#[derive(Debug, Default)]
struct Outcome {
    thinking: String,
    answer: String,
    calls: Vec<Call>,
    usage: Option<(usize, usize)>,
    finish: Option<String>,
    error: Option<String>,
    produced: bool,
}

/// Reads a fixture as `ProviderRun` does (thinking fields between `<think>` tags, the text as
/// sent, the calls at the end), then splits the thinking from the answer as the Engine's chat does
fn read(fixture: &str, compat: &Compat, provider: &str) -> Outcome {
    let mut decoder = Decoder::new(compat);
    let mut engine_text = String::new();
    let mut in_think = false;
    let mut outcome = Outcome::default();
    for line in fixture.lines() {
        let Some(data) = data_of(line) else { continue };
        match decoder.read(data) {
            Read::Done => break,
            Read::Nothing => {}
            Read::Error { message, .. } => {
                outcome.error = Some(message);
                break;
            }
            Read::Piece { thinking, text, .. } => {
                if let Some(thinking) = thinking {
                    if !in_think {
                        engine_text.push_str(THINK_START);
                        in_think = true;
                    }
                    engine_text.push_str(&thinking);
                }
                if let Some(text) = text {
                    if in_think {
                        engine_text.push_str(THINK_END);
                        in_think = false;
                    }
                    engine_text.push_str(&text);
                }
            }
        }
    }
    outcome.produced = decoder.produced();
    let finished = decoder.finish(provider);
    engine_text.push_str(&finished.text);
    if in_think {
        engine_text.push_str(THINK_END);
    }
    for block in &finished.calls {
        engine_text.push('\n');
        engine_text.push_str(block);
    }
    let (thinking, rest) = thinking_and_answer(&engine_text);
    let (answer, calls) = parse_calls(&rest);
    outcome.thinking = thinking;
    outcome.answer = answer;
    outcome.calls = calls;
    outcome.usage = finished.usage;
    outcome.finish = finished.finish;
    outcome
}

/// The text between `<think>` tags, and the rest: what the Engine's chat shows apart
fn thinking_and_answer(text: &str) -> (String, String) {
    let (mut thinking, mut answer, mut rest) = (String::new(), String::new(), text);
    while let Some(start) = rest.find(THINK_START) {
        answer.push_str(&rest[..start]);
        let after = &rest[start + THINK_START.len()..];
        let end = after.find(THINK_END).unwrap_or(after.len());
        thinking.push_str(&after[..end]);
        rest = after.get(end + THINK_END.len()..).unwrap_or("");
    }
    answer.push_str(rest);
    (thinking, answer)
}

fn args(call: &Call) -> Value {
    serde_json::from_str(&call.arguments).unwrap_or(Value::Null)
}

const GEMINI: &str = "https://generativelanguage.googleapis.com/v1beta/openai";

#[test]
fn gemini_parallel_calls_stay_two_and_the_first_keeps_its_signature() {
    let out = read(
        include_str!("fixtures/gemini_parallel_calls.sse"),
        &detect(GEMINI),
        "gemini",
    );
    assert_eq!(out.calls.len(), 2, "{out:?}");
    assert_eq!(
        (
            out.calls[0].name.as_str(),
            args(&out.calls[0])["path"].as_str()
        ),
        ("read_file", Some("notes.txt"))
    );
    assert_eq!(
        (
            out.calls[1].name.as_str(),
            args(&out.calls[1])["path"].as_str()
        ),
        ("list_dir", Some("docs"))
    );
    let first = out.calls[0]
        .provider_data
        .as_ref()
        .expect("the first call keeps what Gemini sent");
    assert_eq!(first["provider"], "gemini");
    assert!(
        first["call"]["extra_content"]["google"]["thought_signature"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    assert!(first["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("call_")));
    assert!(out.thinking.is_empty() && out.answer.is_empty());
    assert_eq!(out.usage.map(|u| u.0), Some(100));
}

#[test]
fn gemini_thoughts_are_thinking_and_the_rest_is_the_answer() {
    let out = read(
        include_str!("fixtures/gemini_thoughts.sse"),
        &detect(GEMINI),
        "gemini",
    );
    assert!(out.thinking.contains("391"), "{out:?}");
    assert!(
        !out.answer.contains("<thought>")
            && !out.answer.contains("</thought>")
            && !out.answer.contains("<think>")
    );
    assert!(
        out.answer.starts_with("No, 391 is not prime"),
        "{}",
        out.answer
    );
    assert!(out.calls.is_empty());
    assert_eq!(out.finish.as_deref(), Some("stop"));
}

#[test]
fn gemini_call_after_thinking() {
    let out = read(
        include_str!("fixtures/gemini_thoughts_then_call.sse"),
        &detect(GEMINI),
        "gemini",
    );
    assert_eq!(out.calls.len(), 1);
    assert_eq!(out.calls[0].name, "list_dir");
    assert!(out.calls[0]
        .provider_data
        .as_ref()
        .is_some_and(|d| d["call"]["extra_content"]["google"]["thought_signature"].is_string()));
}

#[test]
fn groq_thinking_field_then_a_call() {
    let out = read(
        include_str!("fixtures/groq_gpt_oss_call.sse"),
        &detect("https://api.groq.com/openai/v1"),
        "groq",
    );
    assert!(
        out.thinking.contains("notes.txt"),
        "the reasoning field is thinking: {out:?}"
    );
    assert!(out.answer.is_empty());
    assert_eq!(out.calls.len(), 1);
    assert_eq!(
        (
            out.calls[0].name.as_str(),
            args(&out.calls[0])["path"].as_str()
        ),
        ("read_file", Some("notes.txt"))
    );
    // Groq's id comes back to Groq
    assert!(out.calls[0]
        .provider_data
        .as_ref()
        .is_some_and(|d| d["id"].as_str().is_some_and(|id| id.starts_with("fc_"))));
    assert_eq!(out.usage, Some((144, 201)));
    assert_eq!(out.finish.as_deref(), Some("tool_calls"));
}

#[test]
fn openai_call_arguments_in_pieces() {
    let out = read(
        include_str!("fixtures/openai_call_in_pieces.sse"),
        &detect("https://api.openai.com/v1"),
        "openai",
    );
    assert_eq!(out.calls.len(), 1);
    assert_eq!(args(&out.calls[0])["path"], "notes.txt");
    assert_eq!(
        out.calls[0].provider_data.as_ref().map(|d| d["id"].clone()),
        Some(Value::from("call_abc123"))
    );
    assert_eq!(out.usage, Some((82, 99)));
}

#[test]
fn deepseek_reasoning_content_is_thinking() {
    let out = read(
        include_str!("fixtures/deepseek_reasoning_then_answer.sse"),
        &detect("https://api.deepseek.com/v1"),
        "deepseek",
    );
    assert_eq!(out.thinking, "17 times 20 is 340, plus 51 is 391.");
    assert_eq!(out.answer, "391");
    assert_eq!(out.usage, Some((14, 44)));
}

#[test]
fn mistral_two_whole_calls_in_one_event() {
    let out = read(
        include_str!("fixtures/mistral_two_calls.sse"),
        &detect("https://api.mistral.ai/v1"),
        "mistral",
    );
    let names: Vec<&str> = out.calls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["read_file", "list_dir"]);
    assert_eq!(
        out.calls[1].provider_data.as_ref().map(|d| d["id"].clone()),
        Some(Value::from("k8Ra9xQ2p"))
    );
}

#[test]
fn openrouter_comments_are_skipped_and_reasoning_is_thinking() {
    let out = read(
        include_str!("fixtures/openrouter_reasoning.sse"),
        &detect("https://openrouter.ai/api/v1"),
        "openrouter",
    );
    assert_eq!(out.thinking, "The user wants a greeting.");
    assert_eq!(out.answer, "Hello there!");
    assert_eq!(out.usage, Some((12, 32)));
}

#[test]
fn think_tags_in_the_text_are_thinking_even_when_cut() {
    let out = read(
        include_str!("fixtures/think_tags_in_text.sse"),
        &Compat::default(),
        "local",
    );
    assert_eq!(out.thinking.trim(), "Add the tens first.");
    assert_eq!(out.answer, "391");
}

#[test]
fn an_error_after_the_start_is_an_error() {
    let out = read(
        include_str!("fixtures/error_mid_stream.sse"),
        &Compat::default(),
        "p",
    );
    assert_eq!(out.error.as_deref(), Some("Overloaded"));
    assert!(
        out.produced,
        "text had come: the error goes to the person, the request is not repeated"
    );
}

#[test]
fn an_empty_answer_is_seen_as_empty() {
    let out = read(
        include_str!("fixtures/empty_answer.sse"),
        &Compat::default(),
        "p",
    );
    assert!(
        !out.produced,
        "nothing came: the request is asked for once more"
    );
    assert!(out.answer.is_empty() && out.calls.is_empty());
}

#[test]
fn every_fixture_says_where_it_comes_from() {
    for (name, text) in [
        (
            "gemini_parallel_calls",
            include_str!("fixtures/gemini_parallel_calls.sse"),
        ),
        (
            "gemini_thoughts",
            include_str!("fixtures/gemini_thoughts.sse"),
        ),
        (
            "gemini_thoughts_then_call",
            include_str!("fixtures/gemini_thoughts_then_call.sse"),
        ),
        (
            "groq_gpt_oss_call",
            include_str!("fixtures/groq_gpt_oss_call.sse"),
        ),
        (
            "openai_call_in_pieces",
            include_str!("fixtures/openai_call_in_pieces.sse"),
        ),
        (
            "deepseek_reasoning_then_answer",
            include_str!("fixtures/deepseek_reasoning_then_answer.sse"),
        ),
        (
            "mistral_two_calls",
            include_str!("fixtures/mistral_two_calls.sse"),
        ),
        (
            "openrouter_reasoning",
            include_str!("fixtures/openrouter_reasoning.sse"),
        ),
        (
            "think_tags_in_text",
            include_str!("fixtures/think_tags_in_text.sse"),
        ),
        (
            "error_mid_stream",
            include_str!("fixtures/error_mid_stream.sse"),
        ),
        ("empty_answer", include_str!("fixtures/empty_answer.sse")),
        (
            "anthropic_thinking_then_calls",
            include_str!("fixtures/anthropic_thinking_then_calls.sse"),
        ),
        (
            "anthropic_overloaded",
            include_str!("fixtures/anthropic_overloaded.sse"),
        ),
    ] {
        assert!(
            text.starts_with(": "),
            "{name}: the first line is a comment saying where the stream comes from"
        );
    }
}

fn claude() -> cluaiz_core::providers::Target {
    let mut config = cluaiz_core::providers::Provider::new("https://api.anthropic.com/v1");
    config.api_key = Some("sk-test".into());
    cluaiz_core::providers::Target {
        provider: "claude".into(),
        config,
        model: "claude-opus-5-5".into(),
    }
}

#[test]
fn claude_thinking_text_and_calls_with_the_signature_kept() {
    let out = read(
        include_str!("fixtures/anthropic_thinking_then_calls.sse"),
        &Compat::of(&claude()),
        "claude",
    );
    assert_eq!(
        out.thinking,
        "Both files are needed, so I read them together."
    );
    assert_eq!(out.answer, "Reading both.");
    let names: Vec<&str> = out.calls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["read_file", "list_dir"]);
    assert_eq!(args(&out.calls[0])["path"], "notes.txt");
    let data = out.calls[0]
        .provider_data
        .as_ref()
        .expect("the first call keeps the thinking block");
    assert_eq!(data["id"], "toolu_01A");
    assert_eq!(
        data["call"][super::anthropic::THINKING_FIELD][0]["signature"],
        "EqQBCgIYAhIM1gbcDa9GJwZA2b3hGgxBdjrkzLoky3dl1pkiMOYds"
    );
    assert!(out.calls[1]
        .provider_data
        .as_ref()
        .is_some_and(|d| d["call"].get(super::anthropic::THINKING_FIELD).is_none()));
    assert_eq!(out.usage, Some((610, 706)));
    assert_eq!(out.finish.as_deref(), Some("tool_calls"));
}

#[test]
fn claude_gets_its_thinking_block_back_before_its_calls() {
    // The answer as the Engine keeps it, then the next request to the same provider
    let compat = Compat::of(&claude());
    let mut decoder = Decoder::new(&compat);
    for line in include_str!("fixtures/anthropic_thinking_then_calls.sse").lines() {
        if let Some(data) = data_of(line) {
            decoder.read(data);
        }
    }
    let blocks = decoder.finish("claude").calls.join(
        "
",
    );
    let pairs: Vec<(String, String)> = [
        ("user", "Read both"),
        ("assistant", blocks.as_str()),
        ("tool", "MANGO-42"),
        ("tool", "notes.txt"),
    ]
    .iter()
    .map(|(r, c)| (r.to_string(), c.to_string()))
    .collect();
    let openai = super::history::openai_conversation(&pairs, Some(&claude()));
    let (_, messages) = super::anthropic::convert(openai);
    let answer = &messages[1]["content"];
    assert_eq!(answer[0]["type"], "thinking");
    assert_eq!(
        answer[0]["thinking"],
        "Both files are needed, so I read them together."
    );
    assert_eq!(
        answer[0]["signature"],
        "EqQBCgIYAhIM1gbcDa9GJwZA2b3hGgxBdjrkzLoky3dl1pkiMOYds"
    );
    assert_eq!(
        (answer[1]["type"].as_str(), answer[1]["id"].as_str()),
        (Some("tool_use"), Some("toolu_01A"))
    );
    assert_eq!(answer[2]["id"], "toolu_01B");
    let results = &messages[2]["content"];
    assert_eq!(
        (
            results[0]["tool_use_id"].as_str(),
            results[1]["tool_use_id"].as_str()
        ),
        (Some("toolu_01A"), Some("toolu_01B"))
    );
    // Another provider never gets Claude's thinking block
    let groq = cluaiz_core::providers::Target {
        provider: "groq".into(),
        config: cluaiz_core::providers::Provider::new("https://api.groq.com/openai/v1"),
        model: "m".into(),
    };
    let other = super::history::openai_conversation(&pairs, Some(&groq));
    assert!(other[1]["tool_calls"][0]
        .get(super::anthropic::THINKING_FIELD)
        .is_none());
}

#[test]
fn a_claude_error_event_is_an_error() {
    let out = read(
        include_str!("fixtures/anthropic_overloaded.sse"),
        &Compat::of(&claude()),
        "claude",
    );
    assert_eq!(out.error.as_deref(), Some("Overloaded"));
    assert!(!out.produced);
}

#[test]
fn urls_and_formats_of_each_platform() {
    use super::compat::{chat_url, Wire};
    let target = |url: &str, model: &str| cluaiz_core::providers::Target {
        provider: "p".into(),
        config: cluaiz_core::providers::Provider::new(url),
        model: model.into(),
    };
    let vertex_claude = target(
        "https://us-east5-aiplatform.googleapis.com/v1/projects/p1/locations/us-east5",
        "claude-sonnet-5-5",
    );
    assert_eq!(chat_url(&vertex_claude, true), "https://us-east5-aiplatform.googleapis.com/v1/projects/p1/locations/us-east5/publishers/anthropic/models/claude-sonnet-5-5:streamRawPredict");
    assert!(Compat::of(&vertex_claude).vertex);
    let vertex_openai = target(
        "https://aiplatform.googleapis.com/v1/projects/p1/locations/global/endpoints/openapi",
        "google/gemini-3.5-flash",
    );
    assert_eq!(Compat::of(&vertex_openai).wire, Wire::OpenAi);
    assert!(chat_url(&vertex_openai, true).ends_with("/endpoints/openapi/chat/completions"));
    let bedrock_claude = target(
        "https://bedrock-mantle.us-east-1.api.aws/anthropic/v1",
        "anthropic.claude-opus-5-5",
    );
    assert_eq!(
        chat_url(&bedrock_claude, true),
        "https://bedrock-mantle.us-east-1.api.aws/anthropic/v1/messages"
    );
    assert!(!Compat::of(&bedrock_claude).vertex);
    let bedrock_openai = target(
        "https://bedrock-runtime.us-east-1.amazonaws.com/openai/v1",
        "openai.gpt-oss-120b-1:0",
    );
    assert_eq!(Compat::of(&bedrock_openai).wire, Wire::OpenAi);
    assert_eq!(
        chat_url(
            &target("https://api.anthropic.com/v1", "claude-opus-5-5"),
            true
        ),
        "https://api.anthropic.com/v1/messages"
    );
    // A proxy that speaks Anthropic: providers.json's type
    let mut proxy = target("https://llm.example.com/v1", "claude-opus-5-5");
    proxy.config.kind = "anthropic".into();
    assert_eq!(Compat::of(&proxy).wire, Wire::Anthropic);
}
