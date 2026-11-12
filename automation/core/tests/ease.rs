//! The plain explanation and the palette (docs/automation/methods/flows.explain.md, flows.nodes.md).

mod common;

use cluaiz_automation::ease::{self, explain};
use cluaiz_automation::validate::validate;
use common::TestCatalog;
use serde_json::{json, Value};

fn explained(flow: Value) -> Value {
    let report = validate(&flow, None, &TestCatalog);
    explain::explain(&flow, &report, &TestCatalog)
}

#[test]
fn cron_lines_in_words() {
    assert_eq!(explain::cron_words("0 18 * * 5"), "every Friday at 18:00");
    assert_eq!(explain::cron_words("30 9 * * 1-5"), "every Monday to Friday at 09:30");
    assert_eq!(explain::cron_words("0 7 * * *"), "every day at 07:00");
    assert_eq!(explain::cron_words("*/15 * * * *"), "every 15 minutes");
    assert_eq!(explain::cron_words("0 * * * *"), "every hour at :00");
    assert_eq!(explain::cron_words("0 8 1,15 * *"), "on day 1, 15 of every month at 08:00");
    assert_eq!(explain::cron_words("0 8 * 2 *"), "on the schedule `0 8 * 2 *`");
}

#[test]
fn conditions_in_words() {
    let c = json!({ "all": [{ "gt": [{ "var": "steps.order.output.total" }, 5000] }, { "any": [{ "eq": [{ "var": "a" }, "x"] }, { "not": { "var": "b" } }] }] });
    assert_eq!(explain::condition(&c), "steps.order.output.total > 5000 and (a is \"x\" or not (b))");
}

#[test]
fn a_flow_in_words() {
    let flow = json!({
        "flow": "cluaiz.flow/1",
        "name": "weekly-report",
        "timezone": "Asia/Kolkata",
        "on": [{ "schedule": "0 18 * * 5" }, { "file": { "path": "~/Inbox", "glob": "*.pdf", "events": ["created", "changed"] } }],
        "steps": [
            { "id": "notes", "call": "fs.read", "with": { "path": "~/Inbox/a.txt" } },
            { "id": "report", "ask": "Write a short weekly report of these notes.", "with": "${{ steps.notes.output }}" },
            { "if": { "gt": [{ "var": "steps.report.output" }, ""] },
              "then": [{ "tool": "mcp:x/send", "with": { "text": "${{ steps.report.output }}" }, "retry": { "max": 3 } }],
              "else": [{ "fail": "nothing to send" }] }
        ]
    });
    let out = explained(flow);
    assert_eq!(out["triggers"], json!(["Every Friday at 18:00 (Asia/Kolkata)", "When a file matching *.pdf is created or changed in ~/Inbox"]));
    assert_eq!(
        out["summary"],
        "Every Friday at 18:00 (Asia/Kolkata) or when a file matching *.pdf is created or changed in ~/Inbox → call fs.read with path → A model answers “Write a short weekly report of these notes.” → if steps.report.output > \"\"."
    );
    let steps = out["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 5);
    assert_eq!(steps[3], json!({ "id": "_2.then.0", "text": "Run mcp:x/send with text, tried again up to 3 times", "depth": 1, "list": "then" }));
    assert_eq!(steps[4]["text"], "Stop the run with the error “nothing to send”");
    assert_eq!(steps[4]["list"], "else");
    let touches: Vec<&str> = out["touches"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(touches.contains(&"Runs tools: mcp:x/send"), "{touches:?}");
    assert!(touches.contains(&"Reads with: fs.read"), "{touches:?}");
    assert!(touches.contains(&"Sends data out with: mcp:x/send"), "{touches:?}");
    assert!(touches.contains(&"Models: the default chat model"), "{touches:?}");
}

#[test]
fn conditions_and_loops_are_said() {
    let flow = json!({
        "flow": "cluaiz.flow/1",
        "name": "loops",
        "steps": [
            { "id": "items", "set": { "list": [1, 2] } },
            { "each": "${{ vars.list }}", "as": "n", "parallel": 2, "do": [
                { "when": { "gt": [{ "var": "n" }, 1] }, "emit": "big" }
            ] },
            { "wait": { "for": "5m" } },
            { "approve": "Send it?" },
            { "return": { "done": true } }
        ]
    });
    let out = explained(flow);
    let texts: Vec<&str> = out["steps"].as_array().unwrap().iter().map(|s| s["text"].as_str().unwrap()).collect();
    assert_eq!(texts, vec!["Keep list", "For each item of ${{ vars.list }} (as n), 2 at a time", "Only when n > 1: send the event big", "Wait 5m", "Ask a person “Send it?”", "End the run with done"]);
    assert_eq!(out["triggers"], json!(["By hand"]));
}

#[test]
fn the_palette_lists_every_kind() {
    assert_eq!(ease::step_kinds().len(), 17);
    assert!(ease::trigger_kinds().iter().any(|t| t["kind"] == "tool"));
    let gt = ease::operators().into_iter().find(|o| o["name"] == "gt").unwrap();
    assert_eq!(gt, json!({ "name": "gt", "group": "comparison", "arguments": 2, "returns": "bool" }));
}

#[test]
fn confirm_takes_a_mode() {
    let with = |confirm: Value| json!({ "flow": "cluaiz.flow/1", "name": "t", "on": [{ "tool": { "confirm": confirm } }], "steps": [{ "set": { "a": 1 } }] });
    for ok in [json!(true), json!("auto"), json!("strict"), json!("full_access"), json!("sandboxed")] {
        assert!(validate(&with(ok.clone()), None, &TestCatalog).valid, "{ok}");
    }
    assert!(!validate(&with(json!("sometimes")), None, &TestCatalog).valid);
}
