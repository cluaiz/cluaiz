//! Runs real flows against the test host (tests/common): from one step to fifty, branches,
//! loops, retries, resume after a stop, idempotency, large values.

mod common;

use common::*;
use cluaiz_automation::journal::{self, RunStatus};
use cluaiz_automation::run::Runner;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn one_step() {
    let b = bench(TestHost::default());
    let done = run(&b, json!({ "flow": "cluaiz.flow/1", "name": "test", "steps": [{ "id": "r", "call": "fs.read", "with": { "path": "a.txt" } }], "outputs": { "text": "${{ steps.r.output.text }}" } })).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(done.outputs, json!({ "text": "contents of a.txt" }));
    assert_eq!(done.steps_done, 1);
}

#[tokio::test]
async fn read_a_file_ask_a_model_write_a_note() {
    let b = bench(TestHost::default());
    let done = run_with(
        &b,
        request(
            json!({ "flow": "cluaiz.flow/1", "name": "notes", "inputs": { "file": { "type": "string" } },
                "steps": [
                    { "id": "read", "call": "fs.read", "with": { "path": "${{ inputs.file }}" } },
                    { "id": "short", "ask": "Summarise this.", "with": "${{ steps.read.output.text }}", "model": "qwen3-4b" },
                    { "id": "note", "call": "notes.create", "with": { "text": "Summary: ${{ steps.short.output }}" } }
                ],
                "outputs": { "note": "${{ steps.note.output.id }}" } }),
            json!({ "file": "report.pdf" }),
        ),
    )
    .await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    let ask = b.host.asks.lock().unwrap()[0].clone();
    assert!(ask.system.starts_with("You are one step of an automated flow"));
    assert_eq!(ask.user, "Summarise this.\n\nData:\ncontents of report.pdf");
    let (_, note) = b.host.calls.lock().unwrap().last().cloned().unwrap();
    assert_eq!(note["text"], "Summary: summary of: Summarise this.");
    assert_eq!(done.outputs, json!({ "note": "note_1" }));
    let records = steps_of(&b, &done);
    assert_eq!(records[1].tokens, json!({ "prompt": 10, "completion": 5 }));
    assert_eq!(records[1].model.as_deref(), Some("qwen3-4b"));
}

#[tokio::test]
async fn inputs_are_checked_and_defaults_filled() {
    let b = bench(TestHost::default());
    let f = json!({ "flow": "cluaiz.flow/1", "name": "test", "inputs": { "days": { "type": "integer", "default": 7 }, "repo": { "type": "string" } },
        "steps": [{ "set": { "d": "${{ inputs.days }}" } }], "outputs": { "d": "${{ vars.d }}" } });
    let missing = run_with(&b, request(f.clone(), json!({}))).await;
    assert_eq!(missing.status, RunStatus::Failed);
    assert_eq!(missing.error.unwrap().kind, "invalid_input");
    let typo = run_with(&b, request(f.clone(), json!({ "repo": "a", "dyas": 3 }))).await;
    assert!(typo.error.unwrap().message.contains("dyas"));
    let ok = run_with(&b, request(f, json!({ "repo": "a" }))).await;
    assert_eq!(ok.outputs, json!({ "d": 7 }));
}

#[tokio::test]
async fn branches_choose_and_scope_their_steps() {
    let b = bench(TestHost::default());
    let done = run(
        &b,
        json!({ "flow": "cluaiz.flow/1", "name": "test",
            "steps": [
                { "id": "n", "set": { "total": 6200 } },
                { "id": "big", "if": { "gt": [{ "var": "vars.total" }, 5000] }, "then": [{ "id": "inner", "set": { "size": "big" } }], "else": [{ "set": { "size": "small" } }] },
                { "id": "route", "switch": [{ "is": "bug", "do": [{ "set": { "label": "B" } }] }, { "is": ["feature", "idea"], "do": [{ "set": { "label": "E" } }] }], "match": "idea" },
                { "id": "fallback", "switch": [{ "when": false, "do": [{ "set": { "x": 1 } }] }], "default": [{ "set": { "fell": true } }] }
            ],
            "outputs": { "size": "${{ vars.size }}", "label": "${{ vars.label }}", "fell": "${{ vars.fell }}", "branch": "${{ steps.big.branch }}", "inner": "${{ steps.inner.output }}" } }),
    )
    .await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(done.outputs, json!({ "size": "big", "label": "E", "fell": true, "branch": "then", "inner": null }));
}

#[tokio::test]
async fn each_runs_per_item_and_collects_outputs() {
    let b = bench(TestHost::default());
    let done = run(
        &b,
        json!({ "flow": "cluaiz.flow/1", "name": "test",
            "steps": [
                { "id": "rows", "set": { "list": ["a", "b", "c"] } },
                { "id": "loop", "each": "${{ vars.list }}", "as": "row", "do": [
                    { "id": "echo", "tool": "mcp:x/echo", "with": { "row": "${{ row }}", "i": "${{ index }}" } },
                    { "id": "pick", "set": { "last": "${{ steps.echo.output.row }}" } }
                ] },
                { "id": "batches", "each": [1, 2, 3, 4, 5], "batch": 2, "do": [{ "set": { "n": { "expr": { "size": { "var": "items" } } } } }] }
            ],
            "outputs": { "loop": "${{ steps.loop.output }}", "batches": "${{ steps.batches.output }}" } }),
    )
    .await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(done.outputs["loop"], json!([{ "last": "a" }, { "last": "b" }, { "last": "c" }]));
    assert_eq!(done.outputs["batches"], json!([{ "n": 2 }, { "n": 2 }, { "n": 1 }]));
    let records = steps_of(&b, &done);
    let items: Vec<&Value> = records.iter().filter(|r| r.id == "echo").map(|r| &r.item).collect();
    assert_eq!(items, vec![&json!({ "index": 0 }), &json!({ "index": 1 }), &json!({ "index": 2 })]);
}

#[tokio::test]
async fn each_without_stopping_lists_failed_items() {
    let b = bench(TestHost::default());
    let done = run(
        &b,
        json!({ "flow": "cluaiz.flow/1", "name": "test",
            "steps": [{ "id": "loop", "each": [1, 2, 3], "stop_on_error": false, "do": [
                { "if": { "eq": [{ "var": "item" }, 2] }, "then": [{ "fail": "item two is bad" }], "else": [{ "set": { "ok": "${{ item }}" } }] }
            ] }],
            "outputs": { "out": "${{ steps.loop.output }}", "failed": "${{ steps.loop.failed_count }}", "which": "${{ steps.loop.failed[0].index }}" } }),
    )
    .await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(done.outputs, json!({ "out": [{ "ok": 1 }, null, { "ok": 3 }], "failed": 1, "which": 1 }));
}

#[tokio::test]
async fn retries_then_succeeds() {
    let b = bench(TestHost { flaky_left: AtomicUsize::new(2), ..Default::default() });
    let done = run(&b, flow(json!([{ "id": "f", "tool": "mcp:x/flaky", "retry": { "max": 3, "delay": "1ms", "jitter": false } }]))).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    let record = &steps_of(&b, &done)[0];
    assert_eq!(record.attempts.len(), 3);
    assert_eq!(record.attempts[0].error.as_ref().map(|e| e.kind.as_str()), Some("network"));
    // A tool error is not retried by default
    let b = bench(TestHost::default());
    let done = run(&b, flow(json!([{ "tool": "mcp:x/broken", "retry": { "max": 3, "delay": "1ms" } }]))).await;
    assert_eq!((done.status, b.host.calls_to("broken")), (RunStatus::Failed, 1));
}

#[tokio::test]
async fn on_error_continue_handlers_and_the_flows_on_error() {
    let b = bench(TestHost::default());
    let done = run(
        &b,
        json!({ "flow": "cluaiz.flow/1", "name": "test",
            "steps": [
                { "id": "a", "tool": "mcp:x/broken", "on_error": "continue" },
                { "id": "b", "tool": "mcp:x/broken", "on_error": [{ "set": { "why": "${{ error.message }}" } }] },
                { "set": { "after": "${{ steps.a.status }}/${{ steps.b.output.why }}" } }
            ],
            "outputs": { "after": "${{ vars.after }}" } }),
    )
    .await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(done.outputs["after"], "failed/no such repo");
    let b = bench(TestHost::default());
    let done = run(
        &b,
        json!({ "flow": "cluaiz.flow/1", "name": "test",
            "steps": [{ "id": "x", "tool": "mcp:x/broken" }, { "tool": "mcp:x/echo" }],
            "on_error": [{ "call": "notes.create", "with": { "text": "failed at ${{ error.step }}: ${{ error.message }}" } }] }),
    )
    .await;
    assert_eq!(done.status, RunStatus::Failed);
    assert_eq!(done.error.as_ref().map(|e| (e.kind.as_str(), e.step.as_deref())), Some(("tool", Some("x"))));
    assert_eq!(b.host.calls_to("echo"), 0, "steps after the failure do not run");
    let (_, note) = b.host.calls.lock().unwrap().last().cloned().unwrap();
    assert_eq!(note["text"], "failed at x: no such repo");
}

#[tokio::test]
async fn return_and_fail_end_the_run() {
    let b = bench(TestHost::default());
    let done = run(&b, flow(json!([{ "if": true, "then": [{ "return": { "early": true } }] }, { "tool": "mcp:x/echo" }]))).await;
    assert_eq!((done.status, done.outputs.clone()), (RunStatus::Succeeded, json!({ "early": true })));
    assert_eq!(b.host.calls_to("echo"), 0);
    let done = run(&b, flow(json!([{ "fail": "stock is ${{ run.flow }}", "kind": "out_of_stock", "details": { "n": 1 } }]))).await;
    let error = done.error.unwrap();
    assert_eq!((error.kind.as_str(), error.message.as_str(), error.details.clone()), ("out_of_stock", "stock is test", json!({ "n": 1 })));
}

#[tokio::test]
async fn ask_asks_again_until_the_answer_fits_the_schema() {
    let b = bench(TestHost::default());
    let done = run(
        &b,
        json!({ "flow": "cluaiz.flow/1", "name": "test",
            "steps": [{ "id": "label", "ask": "Kind of issue?", "output_schema": { "type": "object", "required": ["kind"], "properties": { "kind": { "enum": ["bug", "feature"] } } } }],
            "outputs": { "kind": "${{ steps.label.output.kind }}" } }),
    )
    .await;
    assert_eq!(done.outputs, json!({ "kind": "bug" }), "{:?}", done.error);
    let asks = b.host.asks.lock().unwrap();
    assert_eq!(asks.len(), 2);
    assert!(asks[1].user.contains("did not have the required JSON shape"), "{}", asks[1].user);
}

#[tokio::test]
async fn permissions_are_checked_at_every_step() {
    let b = bench(TestHost::default());
    let mut r = request(flow(json!([{ "call": "fs.read", "with": { "path": "a" } }, { "tool": "mcp:x/echo" }])), json!({}));
    r.permissions = json!({ "methods": ["fs.read"], "scopes": ["read"] });
    let done = run_with(&b, r).await;
    let error = done.error.unwrap();
    assert_eq!((error.kind.as_str(), error.step.as_deref()), ("permission", Some("_1")));
    let done = run(&b, flow(json!([{ "call": "tokens.create" }]))).await;
    assert_eq!(done.error.unwrap().kind, "not_callable");
}

#[tokio::test]
async fn idempotency_key_runs_a_thing_once() {
    let b = bench(TestHost::default());
    let f = flow(json!([{ "tool": "mcp:mail/send", "with": { "to": "a" }, "idempotency_key": "reply-${{ inputs.message }}" }]));
    let f = json!({ "flow": "cluaiz.flow/1", "name": "test", "inputs": { "message": { "type": "string" } }, "steps": f["steps"] });
    run_with(&b, request(f.clone(), json!({ "message": "m1" }))).await;
    let second = run_with(&b, request(f.clone(), json!({ "message": "m1" }))).await;
    run_with(&b, request(f, json!({ "message": "m2" }))).await;
    assert_eq!(b.host.calls_to("send"), 2, "m1 is sent once, m2 once");
    assert_eq!(steps_of(&b, &second)[0].status, "reused");
}

#[tokio::test]
async fn large_results_become_files_and_paths_read_into_them() {
    let b = bench(TestHost::default());
    let done = run(
        &b,
        json!({ "flow": "cluaiz.flow/1", "name": "test",
            "steps": [{ "id": "big", "tool": "mcp:x/big" }, { "set": { "third": "${{ steps.big.output[2].n }}", "count": { "expr": { "size": { "var": "steps.big.output" } } } } }],
            "outputs": { "third": "${{ vars.third }}" } }),
    )
    .await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(done.outputs["third"], 2);
    let record = &steps_of(&b, &done)[0];
    let file = record.output["$file"].as_str().unwrap();
    assert!(record.output["bytes"].as_u64().unwrap() > 1 << 20);
    assert!(b.paths.flows.join(file).is_file(), "{file}");
}

#[tokio::test]
async fn dry_runs_and_mocks_do_not_change_anything() {
    let b = bench(TestHost::default());
    let mut r = request(flow(json!([{ "id": "read", "call": "fs.read", "with": { "path": "a" } }, { "id": "send", "tool": "mcp:mail/send" }, { "id": "note", "call": "notes.create", "with": { "text": "x" } }])), json!({}));
    r.dry_run = true;
    r.mocks = [("note".to_string(), json!({ "id": "mocked" }))].into_iter().collect();
    let done = run_with(&b, r).await;
    assert_eq!(done.status, RunStatus::Succeeded);
    assert_eq!((b.host.calls_to("fs.read"), b.host.calls_to("send"), b.host.calls_to("notes.create")), (1, 0, 0));
    assert_eq!(steps_of(&b, &done)[2].output, json!({ "id": "mocked" }));
}

#[tokio::test]
async fn needs_change_the_order() {
    let b = bench(TestHost::default());
    let done = run(
        &b,
        json!({ "flow": "cluaiz.flow/1", "name": "test",
            "steps": [
                { "id": "c", "needs": ["a", "b"], "set": { "c": "${{ steps.a.output.a }}${{ steps.b.output.b }}" } },
                { "id": "a", "needs": [], "set": { "a": "1" } },
                { "id": "b", "needs": [], "set": { "b": "2" } }
            ],
            "outputs": { "c": "${{ vars.c }}" } }),
    )
    .await;
    assert_eq!(done.outputs, json!({ "c": "12" }), "{:?}", done.error);
}

#[tokio::test]
async fn step_timeouts_and_cancel() {
    let b = bench(TestHost { slow_ms: 2_000, ..Default::default() });
    let done = run(&b, flow(json!([{ "tool": "mcp:x/slow", "timeout": "50ms" }]))).await;
    assert_eq!(done.error.unwrap().kind, "timeout");
    let runner = Runner::new(b.host.clone(), b.paths.clone());
    let created = runner.create(request(flow(json!([{ "tool": "mcp:x/slow" }, { "tool": "mcp:x/echo" }])), json!({}))).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        flag.store(true, Ordering::SeqCst);
    });
    let done = runner.execute(created, cancel, no_events()).await;
    assert_eq!(done.status, RunStatus::Cancelled);
    assert_eq!(b.host.calls_to("echo"), 0);
}

#[tokio::test]
async fn a_run_stopped_mid_way_goes_on_without_repeating_finished_steps() {
    let b = bench(TestHost { slow_ms: 60_000, ..Default::default() });
    let f = flow(json!([
        { "id": "first", "tool": "mcp:x/echo", "with": { "n": 1 } },
        { "id": "mail", "tool": "mcp:x/slow", "idempotency_key": "mail-1" },
        { "id": "last", "set": { "done": true } }
    ]));
    let runner = Runner::new(b.host.clone(), b.paths.clone());
    let created = runner.create(request(f, json!({}))).unwrap();
    let run_id = created.run_id().to_string();
    // Automation stops while `mail` is mid-call: the future is dropped, as in a crash
    let stopped = tokio::time::timeout(Duration::from_millis(300), runner.execute(created, Arc::new(AtomicBool::new(false)), no_events())).await;
    assert!(stopped.is_err());
    let dir = b.paths.run_dir("test", &run_id);
    assert_eq!(journal::read_run(&dir).unwrap().status, RunStatus::Running);
    // On restart the owner is asked about the step with a key; they say do not repeat it
    b.host.owner_says.store(false, Ordering::SeqCst);
    let reopened = runner.reopen(dir.clone(), serde_json::from_str(ALL).unwrap()).unwrap();
    let done = runner.execute(reopened, Arc::new(AtomicBool::new(false)), no_events()).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(done.attempt, 1);
    assert_eq!(b.host.calls_to("echo"), 1, "the finished step is not run again");
    assert_eq!(b.host.calls_to("slow"), 1, "the mid-call step is not repeated");
    assert_eq!(b.host.owner_asked.load(Ordering::SeqCst), 1);
    let statuses: Vec<String> = steps_of(&b, &done).into_iter().map(|r| format!("{}:{}", r.id, r.status)).collect();
    assert_eq!(statuses, vec!["first:succeeded", "mail:unknown", "last:succeeded"]);
}

#[tokio::test]
async fn changed_compares_with_the_last_succeeded_run() {
    let b = bench(TestHost::default());
    let f = json!({ "flow": "cluaiz.flow/1", "name": "test", "inputs": { "price": { "type": "integer" } },
        "steps": [{ "set": { "moved": { "expr": { "changed": "inputs.price" } }, "was": { "expr": { "previous": "inputs.price" } } } }],
        "outputs": { "moved": "${{ vars.moved }}", "was": "${{ vars.was }}" } });
    let first = run_with(&b, request(f.clone(), json!({ "price": 10 }))).await;
    let same = run_with(&b, request(f.clone(), json!({ "price": 10 }))).await;
    let moved = run_with(&b, request(f, json!({ "price": 12 }))).await;
    assert_eq!(first.outputs, json!({ "moved": true, "was": null }));
    assert_eq!(same.outputs, json!({ "moved": false, "was": 10 }));
    assert_eq!(moved.outputs, json!({ "moved": true, "was": 10 }));
}

#[tokio::test]
async fn parts_not_built_yet_say_so() {
    let b = bench(TestHost::default());
    let done = run(&b, flow(json!([{ "agent": "look around", "tools": ["mcp:x/echo"], "tool_search": true }]))).await;
    let error = done.error.unwrap();
    assert_eq!(error.kind, "unavailable");
    assert!(error.message.contains("not built yet"), "{}", error.message);
}

#[tokio::test]
async fn fifty_steps() {
    let b = bench(TestHost::default());
    let mut steps: Vec<Value> = vec![json!({ "id": "s0", "set": { "n": 0 } })];
    for i in 1..50 {
        steps.push(json!({ "id": format!("s{i}"), "set": { "n": { "expr": { "add": [{ "var": "vars.n" }, 1] } } } }));
    }
    let done = run(&b, json!({ "flow": "cluaiz.flow/1", "name": "test", "steps": steps, "outputs": { "n": "${{ vars.n }}" } })).await;
    assert_eq!((done.status, done.outputs.clone(), done.steps_done), (RunStatus::Succeeded, json!({ "n": 49 }), 50));
    let report = cluaiz_automation::validate::validate(&json!({ "flow": "cluaiz.flow/1", "name": "test", "steps": steps }), Some("test"), &cluaiz_automation::host::NoCatalog);
    assert!(report.valid, "{:?}", report.problems);
}
