use super::*;
use serde_json::json;

fn codes(flow: Value) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    check_flow(&flow, &mut out);
    out.into_iter().map(|p| (p.path, p.code)).collect()
}

fn base(steps: Value) -> Value {
    json!({ "flow": "cluaiz.flow/1", "name": "t", "steps": steps })
}

#[test]
fn a_good_flow_has_no_problems() {
    let flow = json!({
        "flow": "cluaiz.flow/1", "name": "weekly-issues", "timezone": "Asia/Kolkata",
        "inputs": { "repo": { "type": "string", "default": "my/repo" } },
        "on": [{ "schedule": "0 18 * * 5", "catch_up": "once" }, { "webhook": { "secret": "env:HOOK" } }, { "every": "15m" }],
        "permissions": { "tools": ["mcp:github/*"], "scopes": ["read"] },
        "limits": { "timeout": "10m" }, "concurrency": "skip",
        "steps": [
            { "id": "closed", "tool": "mcp:github/list_issues", "with": { "repo": "${{ inputs.repo }}" }, "retry": { "max": 3 } },
            { "id": "big", "if": { "gt": [{ "size": { "var": "steps.closed.output" } }, 5] }, "then": [{ "set": { "many": true } }] },
            { "each": "${{ steps.closed.output }}", "parallel": 2, "do": [{ "call": "notes.create", "with": { "text": "${{ item.title }}" } }] },
            { "ask": "Summarise", "output_schema": { "type": "object" }, "on_error": "continue" }
        ],
        "ui": { "editor": { "x": 1 } }
    });
    assert_eq!(codes(flow), vec![]);
}

#[test]
fn unknown_and_missing_fields() {
    let problems = codes(json!({ "flow": "cluaiz.flow/1", "name": "t", "stpes": [] }));
    assert!(problems.contains(&("".into(), "missing_field")));
    assert!(problems.contains(&("/stpes".into(), "unknown_field")));
    let mut out = Vec::new();
    check_flow(&base(json!([{ "tool": "x", "wiht": {} }])), &mut out);
    assert_eq!(out[0].fix, json!("with"));
}

#[test]
fn kinds() {
    assert_eq!(codes(base(json!([{ "id": "a" }]))), vec![("/steps/0".into(), "no_kind")]);
    assert_eq!(codes(base(json!([{ "tool": "a", "ask": "b" }]))), vec![("/steps/0".into(), "many_kinds")]);
    // `parallel` of each and `wait` of run are fields, not second kinds
    assert_eq!(codes(base(json!([{ "each": [1], "parallel": 2, "do": [{ "set": {} }] }, { "run": "other", "wait": false }]))), vec![]);
}

#[test]
fn values() {
    let problems = codes(base(json!([{ "tool": "x", "timeout": "10", "on_error": "maybe", "id": "9a" }])));
    assert!(problems.contains(&("/steps/0/timeout".into(), "bad_duration")));
    assert!(problems.contains(&("/steps/0/on_error".into(), "invalid_value")));
    assert!(problems.contains(&("/steps/0/id".into(), "invalid_id")));
    assert_eq!(codes(json!({ "flow": "cluaiz.flow/9", "name": "Bad Name", "steps": [{ "set": {} }] })).len(), 2);
    assert_eq!(codes(base(json!([{ "repeat": [{ "set": {} }], "until": true }])))[0].1, "repeat_without_max");
}

#[test]
fn expressions_and_references() {
    let problems = codes(base(json!([
        { "tool": "x", "when": { "gtt": [1, 2] } },
        { "tool": "x", "when": 1 },
        { "tool": "x", "with": { "key": "${{ env.TOKEN }}", "n": { "expr": { "between": [1, 2] } } } },
        { "tool": "x", "with": { "a": "${{ steps..x }}" } }
    ])));
    assert_eq!(
        problems,
        vec![
            ("/steps/0/when".to_string(), "unknown_operator"),
            ("/steps/1/when".to_string(), "wrong_type"),
            ("/steps/2/with/key".to_string(), "secret_in_text"),
            ("/steps/2/with/n/expr/between".to_string(), "bad_arguments"),
            ("/steps/3/with/a".to_string(), "invalid_value"),
        ]
    );
}

#[test]
fn triggers() {
    let flow = json!({ "flow": "cluaiz.flow/1", "name": "t", "steps": [{ "set": {} }],
        "on": [{ "webhook": {} }, { "schedule": "0 25 * * *" }, { "schedule": "0 9 * * mon-fri", "timezone": "Nowhere/City" }, { "file": { "path": "~/in" }, "dedupe_for": "1h" }] });
    let problems = codes(flow);
    assert!(problems.contains(&("/on/0/webhook".into(), "webhook_without_secret")));
    assert!(problems.contains(&("/on/1/schedule".into(), "bad_cron")));
    assert!(problems.contains(&("/on/2/timezone".into(), "bad_timezone")));
    assert_eq!(problems.len(), 3, "{problems:?}");
}

#[test]
fn switch_forms() {
    let problems = codes(base(json!([
        { "switch": [{ "when": true, "do": [{ "set": {} }] }] },
        { "switch": [{ "is": "bug", "do": [{ "set": {} }] }], "match": "${{ inputs.kind }}" },
        { "switch": [{ "is": "bug", "do": [{ "set": {} }] }] }
    ])));
    assert_eq!(problems, vec![("/steps/2/switch/0".to_string(), "invalid_value")]);
}
