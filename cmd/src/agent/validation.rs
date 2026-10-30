//! Validation around the existing tool parser, before history or execution is changed.

use cluaiz_core::tool_calls;

pub fn calls(answer: &str, stop: &str) -> Result<(String, Vec<(String, String)>), &'static str> {
    let (said, calls) = tool_calls::split_calls(answer);
    if said.contains(tool_calls::OPEN)
        || calls.iter().any(|(_, args)| {
            !serde_json::from_str::<serde_json::Value>(args).is_ok_and(|value| value.is_object())
        })
    {
        return Err("Incomplete or invalid tool call; nothing executed. The invalid call was not added to history.");
    }
    if stop == "limit" && !calls.is_empty() {
        return Err("Generation limit reached while preparing tool calls; nothing executed. Retry with a smaller operation or more available context.");
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
            assert!(calls(text, "eos").is_err());
        }
    }
    #[test]
    fn test_limit_blocks_complete_calls_and_plain_text_remains_valid() {
        let call = tool_calls::canonical_call("read_file", r#"{"path":"file"}"#);
        assert!(calls(&call, "limit").is_err());
        assert_eq!(calls(&call, "eos").unwrap().1.len(), 1);
        assert_eq!(
            calls("ordinary answer", "limit").unwrap().0,
            "ordinary answer"
        );
    }
}
