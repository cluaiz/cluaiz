use serde_json::Value;

/// Parsed representation of an LLM tool call extracted from text/stream accumulator
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedToolCall {
    pub raw_json: String,
    pub raw_fn_name: String,
    pub payload_str: String,
}

/// Robust extraction of `<tool_call>` fragments from a token stream or completed turn.
/// Handles markdown fences, nested JSON objects, and malformed key fallback searches.
pub fn extract_tool_call(text: &str) -> Option<ExtractedToolCall> {
    let start_tag = "<tool_call>";
    let end_tag = "</tool_call>";

    if !text.contains(start_tag) || !text.contains(end_tag) {
        return None;
    }

    let call_start_idx = text.rfind(start_tag)?;
    if !text.is_char_boundary(call_start_idx) {
        return None;
    }
    let clean_token = &text[call_start_idx..];

    let start_pos = clean_token.find(start_tag)? + start_tag.len();
    let end_pos = clean_token.find(end_tag)?;

    if start_pos > end_pos || !clean_token.is_char_boundary(start_pos) || !clean_token.is_char_boundary(end_pos) {
        return None;
    }
    let raw_json = clean_token[start_pos..end_pos].trim();

    // Strip markdown fences (e.g. ```json ... ```) if model wrapped tool call
    let mut sanitized_json = raw_json;
    if let Some(stripped) = sanitized_json.strip_prefix("```json") {
        sanitized_json = stripped;
    } else if let Some(stripped) = sanitized_json.strip_prefix("```") {
        sanitized_json = stripped;
    }
    if let Some(stripped) = sanitized_json.strip_suffix("```") {
        sanitized_json = stripped;
    }
    let sanitized_json = sanitized_json.trim();

    let parsed_json: Value = serde_json::from_str(sanitized_json)
        .or_else(|_| serde_json::from_str(raw_json))
        .unwrap_or_else(|_| serde_json::json!({ "raw": raw_json }));

    let mut extracted_name = parsed_json
        .get("name")
        .or_else(|| parsed_json.get("function"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Fallback key search if json parse failed or key was missing
    if extracted_name.is_none() || extracted_name.as_deref() == Some("unknown") {
        for key in &["\"name\":", "\"function\":", "\"tool\":"] {
            if let Some(pos) = raw_json.find(key) {
                let start = pos + key.len();
                if raw_json.is_char_boundary(start) {
                    let after = raw_json[start..].trim_start();
                    if let Some(stripped) = after.strip_prefix('"') {
                        if let Some(end_quote) = stripped.find('"') {
                            if stripped.is_char_boundary(end_quote) {
                                extracted_name = Some(stripped[..end_quote].to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    let raw_fn_name = extracted_name.unwrap_or_else(|| "unknown".to_string());

    let args_val = parsed_json
        .get("arguments")
        .or_else(|| parsed_json.get("parameters"))
        .cloned()
        .unwrap_or(Value::Null);

    let payload_str = if args_val.is_string() {
        args_val.as_str().unwrap().to_string()
    } else if !args_val.is_null() {
        serde_json::to_string(&args_val).unwrap_or_default()
    } else {
        serde_json::to_string(&parsed_json).unwrap_or_default()
    };

    Some(ExtractedToolCall {
        raw_json: raw_json.to_string(),
        raw_fn_name,
        payload_str,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_clean_tool_call() {
        let input = "I will check the weather.\n<tool_call>{\"name\": \"get_weather\", \"arguments\": {\"city\": \"London\"}}</tool_call>";
        let call = extract_tool_call(input).expect("Should extract tool call");
        assert_eq!(call.raw_fn_name, "get_weather");
        assert!(call.payload_str.contains("London"));
    }

    #[test]
    fn test_extract_markdown_fenced_tool_call() {
        let input = "<tool_call>\n```json\n{\"name\": \"search\", \"arguments\": \"rust lang\"}\n```\n</tool_call>";
        let call = extract_tool_call(input).expect("Should extract fenced tool call");
        assert_eq!(call.raw_fn_name, "search");
        assert_eq!(call.payload_str, "rust lang");
    }

    #[test]
    fn test_no_tool_call() {
        let input = "Just standard conversational text with no tags.";
        assert!(extract_tool_call(input).is_none());
    }
}
