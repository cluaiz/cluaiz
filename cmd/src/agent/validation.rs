//! Validation around the existing tool parser, before history or execution is changed.

use cluaiz_core::tool_calls;

/// Why the calls of an answer are not run
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// A call that cannot be read: the call as written (from its opening tag on, or its name and
    /// arguments when they are not a JSON object). The model is told and asked again.
    Unreadable(String),
    /// The generation limit cut the answer while it held calls
    Limit,
}

impl Refused {
    pub fn message(&self) -> &'static str {
        match self {
            Refused::Unreadable(_) => "Incomplete or invalid tool call; nothing executed. The invalid call was not added to history.",
            Refused::Limit => "Generation limit reached while preparing tool calls; nothing executed. Retry with a smaller operation or more available context.",
        }
    }
}

pub fn calls(answer: &str, stop: &str) -> Result<(String, Vec<tool_calls::Call>), Refused> {
    let (said, calls) = tool_calls::parse_calls(answer);
    if let Some(at) = said.find(tool_calls::OPEN) {
        return Err(Refused::Unreadable(said[at..].to_owned()));
    }
    if let Some(call) = calls
        .iter()
        .find(|call| !serde_json::from_str::<serde_json::Value>(&call.arguments).is_ok_and(|value| value.is_object()))
    {
        return Err(Refused::Unreadable(format!("{} {}", call.name, call.arguments)));
    }
    if stop == "limit" && !calls.is_empty() {
        return Err(Refused::Limit);
    }
    Ok((said, calls))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_incomplete_and_invalid_calls_are_rejected_before_history() {
        for text in [
            "<tool_call>{\"name\":\"write_file\",\"arguments\":",
            "<tool_call>{\"name\":\"write_file\",\"arguments\":\"broken\"}</tool_call>",
            "<tool_call>not JSON</tool_call>",
        ] {
            assert!(matches!(calls(text, "eos"), Err(Refused::Unreadable(_))), "{text}");
        }
    }
    #[test]
    fn an_unreadable_call_is_quoted_from_its_opening_tag() {
        let text = "Let me look.\n<tool_call>\n<function=list_dir>\n<parameter=path>src</parameter>\n</function>\n</tool_call>";
        match calls(text, "eos") {
            Err(Refused::Unreadable(call)) => assert!(call.starts_with(tool_calls::OPEN) && call.contains("list_dir"), "{call}"),
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn test_limit_blocks_complete_calls_and_plain_text_remains_valid() {
        let call = tool_calls::canonical_call("read_file", r#"{"path":"file"}"#);
        assert_eq!(calls(&call, "limit").unwrap_err(), Refused::Limit);
        assert_eq!(calls(&call, "eos").unwrap().1.len(), 1);
        assert_eq!(
            calls("ordinary answer", "limit").unwrap().0,
            "ordinary answer"
        );
    }
}
