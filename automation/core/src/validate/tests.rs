use super::*;
use crate::host::{NoCatalog, NodeInfo};
use serde_json::json;

/// The errors only (warnings are checked in their own tests)
fn problems(flow: Value) -> Vec<(&'static str, String)> {
    validate(&flow, None, &NoCatalog).problems.into_iter().filter(|p| p.is_error()).map(|p| (p.code, p.path)).collect()
}

fn flow(steps: Value) -> Value {
    json!({ "flow": "cluaiz.flow/1", "name": "t", "steps": steps })
}

#[test]
fn ids_and_references() {
    let found = problems(flow(json!([
        { "id": "a", "tool": "mcp:x/one" },
        { "id": "a", "tool": "mcp:x/two" },
        { "id": "c", "ask": "Use ${{ steps.sumary.output }}" },
        { "id": "d", "ask": "Use ${{ steps.later.output }}" },
        { "id": "later", "set": { "x": 1 } }
    ])));
    assert!(found.contains(&("duplicate_id", "/steps/1/id".into())), "{found:?}");
    assert!(found.contains(&("unknown_step", "/steps/2/ask".into())));
    assert!(found.contains(&("not_reachable", "/steps/3/ask".into())));
}

#[test]
fn branches_and_loops_scope_their_steps() {
    let found = problems(flow(json!([
        { "id": "check", "if": true, "then": [{ "id": "inner", "set": { "x": 1 } }, { "id": "uses_inner", "ask": "${{ steps.inner.output }}" }] },
        { "id": "after", "ask": "${{ steps.inner.output }}" },
        { "id": "loop", "each": [1, 2], "as": "row", "do": [{ "id": "per", "ask": "Row ${{ row }} at ${{ index }}" }] },
        { "id": "outside", "ask": "${{ item }}" },
        { "id": "uses_loop", "ask": "${{ steps.loop.output }} and ${{ steps.check.output }}" }
    ])));
    assert_eq!(found, vec![("not_reachable", "/steps/1/ask".into()), ("invalid_value", "/steps/3/ask".into())]);
}

#[test]
fn needs_and_cycles() {
    let parallel = problems(flow(json!([
        { "id": "a", "tool": "mcp:x/one" },
        { "id": "b", "needs": [], "tool": "mcp:x/two", "with": { "v": "${{ steps.a.output }}" } },
        { "id": "c", "needs": ["a", "b"], "ask": "${{ steps.a.output }} ${{ steps.b.output }}" }
    ])));
    assert_eq!(parallel, vec![("not_reachable", "/steps/1/with".into())]);
    let cycle = problems(flow(json!([
        { "id": "a", "needs": ["b"], "set": {} },
        { "id": "b", "needs": ["a"], "set": {} },
        { "id": "c", "needs": ["nope"], "set": {} }
    ])));
    assert!(cycle.iter().any(|(code, _)| *code == "cycle"), "{cycle:?}");
    assert!(cycle.contains(&("unknown_step", "/steps/2/needs/0".into())));
}

#[test]
fn on_error_reads_error_and_output_reads_result() {
    let found = problems(json!({ "flow": "cluaiz.flow/1", "name": "t",
        "steps": [{ "id": "fetch", "tool": "mcp:shop/orders", "output": "${{ result.items }}", "on_error": [{ "set": { "why": "${{ error.message }}" } }] }],
        "on_error": [{ "ask": "Failed at ${{ error.step }} after ${{ steps.fetch.status }}" }],
        "outputs": { "orders": "${{ steps.fetch.output }}" } }));
    assert_eq!(found, vec![]);
}

#[test]
fn secrets() {
    let found = problems(json!({ "flow": "cluaiz.flow/1", "name": "t", "secrets": ["SHOP"],
        "on": [{ "webhook": { "secret": "env:OTHER" } }],
        "steps": [{ "tool": "mcp:x/y", "settings": { "key": "env:SHOP" }, "with": { "token": "env:SHOP" } }] }));
    assert_eq!(found, vec![("unknown_secret", "/on/0/webhook/secret".into()), ("secret_in_text", "/steps/0/with/token".into())]);
}

#[test]
fn permissions_needed_and_missing() {
    let report = validate(
        &json!({ "flow": "cluaiz.flow/1", "name": "t", "permissions": { "tools": ["mcp:github/*"] },
            "steps": [{ "tool": "mcp:github/list" }, { "call": "fs.read", "with": { "path": "~/Notes/a.md" } }, { "ask": "x" }, { "call": "tokens.create" }] }),
        None,
        &NoCatalog,
    );
    assert_eq!(report.permissions_needed, json!({ "tools": ["mcp:github/list"], "methods": ["fs.read"], "folders": ["~/Notes/a.md"], "models": ["*"], "scopes": ["exec"] }));
    assert_eq!(report.permissions_missing, json!({ "methods": ["fs.read"], "folders": ["~/Notes/a.md"], "models": ["*"], "scopes": ["exec"] }));
    let codes: Vec<&str> = report.problems.iter().map(|p| p.code).collect();
    assert!(codes.contains(&"not_callable") && codes.contains(&"permission_needed"), "{codes:?}");
    assert!(!report.valid);
}

struct Installed;
impl Catalog for Installed {
    fn tool(&self, name: &str) -> Option<NodeInfo> {
        (name == "mcp:github/create_issue").then(|| NodeInfo {
            input_schema: Some(json!({ "type": "object", "required": ["title"], "properties": { "title": { "type": "string" }, "query": { "type": "string", "x-cluaiz-code": true } } })),
            network: true,
            sends: true,
            ..Default::default()
        })
    }
    fn method(&self, name: &str) -> Option<NodeInfo> {
        (name == "fs.read").then(|| NodeInfo { scope: Some("read".into()), read_only: true, ..Default::default() })
    }
    fn names(&self) -> Vec<String> {
        vec!["mcp:github/create_issue".into(), "fs.read".into()]
    }
    fn model_installed(&self, model: &str) -> Option<bool> {
        Some(model == "qwen3-4b")
    }
}

#[test]
fn nodes_against_what_is_installed() {
    let report = validate(
        &json!({ "flow": "cluaiz.flow/1", "name": "t", "on": [{ "webhook": { "secret": "env:H" } }], "secrets": ["H"],
            "steps": [
                { "tool": "mcp:github/create_isue" },
                { "tool": "mcp:github/create_issue", "with": { "query": "x ${{ trigger.body.q }}" } },
                { "call": "fs.raed" },
                { "id": "unused", "call": "fs.read", "with": { "path": "a" } },
                { "ask": "x", "model": "llama-big" }
            ] }),
        Some("other"),
        &Installed,
    );
    let codes: Vec<(&str, &str)> = report.problems.iter().map(|p| (p.code, p.path.as_str())).collect();
    for expected in [
        ("name_mismatch", "/name"),
        ("unknown_node", "/steps/0/tool"),
        ("invalid_input", "/steps/1/with/title"),
        ("no_retry_network", "/steps/1"),
        ("no_idempotency", "/steps/1"),
        ("reference_in_code", "/steps/1/with/query"),
        ("unknown_node", "/steps/2/call"),
        ("unused_output", "/steps/3"),
        ("model_not_installed", "/steps/4/model"),
    ] {
        assert!(codes.contains(&expected), "{expected:?} not in {codes:?}");
    }
    assert_eq!(report.problems.iter().find(|p| p.path == "/steps/0/tool").map(|p| p.fix.clone()), Some(json!("mcp:github/create_issue")));
    assert!(report.outside_content);
}

#[test]
fn normalized_and_hash() {
    let report = validate(&flow(json!([{ "set": {} }])), None, &NoCatalog);
    assert!(report.valid);
    assert_eq!(report.normalized["concurrency"], "queue");
    assert_eq!(report.normalized["limits"]["max_parallel"], 4);
    assert!(report.hash.starts_with("sha256:") && report.hash.len() == 71);
}
