//! Fewer Engine tools for small models (context policy `small_model_tools`).
//!
//! A model under [`SMALL_MODEL_PARAMS`] weights gets only the client's own tools, the tools bound to
//! the chat, and tool search (`find_tools`, `load_tool`, `unload_tool`, `read_tool_file`). The
//! Engine's other functions (`read_item`, `recall`, `open_memory`, `remember`, `forget`,
//! `task_update`, saved flows) are left out of the request.
//!
//! Evidence: run 77 (`test/cloud/runs/2026-11-25-run77-small-model-tool-count`, partial). Llama 3.2
//! 1B scored 32/40 with only the five client tools and 6/40 when the Engine's context tools came
//! along (`read_item` repeated on 29 of 40 prompts, 414 continuation steps). Qwen3 0.6B (thinking
//! off) went from 27/40 to 21/40. The run did not isolate the count of schemas from the route's
//! prompt policy, and Llama 3.2 3B (above the line) also looped (11 prompts, 248 steps), so the line
//! is a default that `small_model_tools: "full"` turns off, not a proven boundary.

/// Weights below which a model counts as small: 2 billion, as counted by llama.cpp
/// (`llama_model_n_params`, every tensor element of the loaded model, embeddings included)
pub const SMALL_MODEL_PARAMS: u64 = 2_000_000_000;

/// True when this request leaves the Engine's other functions out: the policy is `"lean"` and the
/// loaded model has fewer than [`SMALL_MODEL_PARAMS`] weights. An unknown count (a provider's
/// model, ONNX, nothing loaded) or 0 never reduces anything.
pub fn lean(setting: &str, n_params: Option<u64>) -> bool {
    setting == "lean" && n_params.is_some_and(|n| n > 0 && n < SMALL_MODEL_PARAMS)
}

/// Function names of `schemas` (OpenAI `{"type": "function", "function": {"name": ...}}`)
pub fn function_names(schemas: &[serde_json::Value]) -> Vec<String> {
    schemas.iter().filter_map(|schema| schema["function"]["name"].as_str().map(str::to_owned)).collect()
}

/// The `small_model` part of `context_report.tools`
pub fn report(setting: &str, n_params: Option<u64>, applied: bool, left_out: &[String]) -> serde_json::Value {
    serde_json::json!({
        "setting": setting,
        "params": n_params,
        "threshold_params": SMALL_MODEL_PARAMS,
        "applied": applied,
        "left_out": left_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_model_under_two_billion_weights_gets_the_lean_tool_set() {
        assert!(lean("lean", Some(600_000_000)));
        assert!(lean("lean", Some(1_700_000_000)));
        assert!(lean("lean", Some(SMALL_MODEL_PARAMS - 1)));
    }

    #[test]
    fn two_billion_weights_and_more_keep_every_tool() {
        assert!(!lean("lean", Some(SMALL_MODEL_PARAMS)));
        assert!(!lean("lean", Some(3_000_000_000)));
        assert!(!lean("lean", Some(4_000_000_000)));
    }

    #[test]
    fn an_unknown_or_zero_count_never_reduces_tools() {
        assert!(!lean("lean", None));
        assert!(!lean("lean", Some(0)));
    }

    #[test]
    fn small_model_tools_full_turns_the_rule_off() {
        assert!(!lean("full", Some(600_000_000)));
    }

    /// The functions a small model keeps: tool search is `ToolSearch::meta_schemas`, which the
    /// rule never touches
    const TOOL_SEARCH_FUNCTIONS: [&str; 4] = [
        cluaiz_agents::LOAD_TOOL_NAME,
        cluaiz_agents::UNLOAD_TOOL_NAME,
        cluaiz_agents::READ_TOOL_FILE_NAME,
        cluaiz_agents::FIND_TOOLS_NAME,
    ];

    #[test]
    fn tool_search_functions_are_the_meta_schemas() {
        let mut meta = function_names(&cluaiz_agents::ToolSearch::meta_schemas());
        meta.sort();
        let mut kept: Vec<String> = TOOL_SEARCH_FUNCTIONS.iter().map(|n| n.to_string()).collect();
        kept.sort();
        assert_eq!(meta, kept);
    }

    #[test]
    fn the_report_names_what_was_left_out() {
        let names = function_names(&[json!({"type": "function", "function": {"name": "read_item"}}), json!({"type": "function"})]);
        assert_eq!(names, vec!["read_item".to_string()]);
        let report = report("lean", Some(1_000), true, &names);
        assert_eq!(report["applied"], json!(true));
        assert_eq!(report["threshold_params"], json!(SMALL_MODEL_PARAMS));
        assert_eq!(report["left_out"], json!(["read_item"]));
        assert_eq!(report["params"], json!(1_000));
    }
}
