//! The steps of the third part of Automation against the test host (tests/common): steps that run
//! together, loops, waits, approvals, events, other flows, WebAssembly and agents.

mod common;

use cluaiz_automation::host::AskAnswer;
use cluaiz_automation::journal::{self, RunStatus};
use cluaiz_automation::run::Runner;
use common::*;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn slow(ms: u64) -> TestHost {
    TestHost { slow_ms: ms, ..Default::default() }
}

#[tokio::test]
async fn parallel_branches_run_together_and_collect_by_name() {
    let b = bench(slow(300));
    let started = Instant::now();
    let done = run(
        &b,
        flow(json!([
            { "id": "both", "parallel": {
                "home": [{ "tool": "mcp:x/slow", "with": { "n": 1 } }],
                "health": [{ "tool": "mcp:x/slow", "with": { "n": 2 } }, { "set": { "checked": true } }] } },
            { "set": { "home": "${{ steps.both.output.home }}" } }
        ])),
    )
    .await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(b.host.most_at_once.load(Ordering::SeqCst), 2, "the branches did not run together");
    assert!(started.elapsed() < Duration::from_secs(5));
    let records = steps_of(&b, &done);
    let both = records.iter().find(|r| r.id == "both").unwrap();
    assert_eq!(both.output, json!({ "home": "late 1", "health": { "checked": true } }));
}

#[tokio::test]
async fn parallel_any_takes_the_first_and_failures_stop_or_not() {
    let b = bench(slow(400));
    let done = run(
        &b,
        flow(json!([{ "id": "race", "parallel": { "fast": [{ "set": { "won": "fast" } }], "slow": [{ "tool": "mcp:x/slow" }] }, "wait": "any" }])),
    )
    .await;
    assert_eq!(done.status, RunStatus::Succeeded);
    let race = steps_of(&b, &done).into_iter().find(|r| r.id == "race").unwrap();
    assert_eq!(race.output, json!({ "fast": { "won": "fast" } }));

    let failing = flow(json!([{ "id": "p", "parallel": { "bad": [{ "tool": "mcp:x/broken" }], "good": [{ "set": { "ok": 1 } }] } }]));
    let done = run(&b, failing).await;
    assert_eq!(done.error.unwrap().kind, "tool");
    let lenient = flow(json!([{ "id": "p", "parallel": { "bad": [{ "tool": "mcp:x/broken" }], "good": [{ "set": { "ok": 1 } }] }, "stop_on_error": false }]));
    let done = run(&b, lenient).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    let p = steps_of(&b, &done).into_iter().find(|r| r.id == "p").unwrap();
    assert_eq!(p.output, json!({ "bad": null, "good": { "ok": 1 } }));
}

#[tokio::test]
async fn max_parallel_holds_for_the_whole_run() {
    let b = bench(slow(150));
    let f = json!({ "flow": "cluaiz.flow/1", "name": "test", "limits": { "max_parallel": 2 },
        "steps": [{ "id": "all", "each": [1, 2, 3, 4, 5, 6], "parallel": 6, "do": [{ "tool": "mcp:x/slow", "with": { "n": "${{ item }}" } }] }] });
    let done = run(&b, f).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(b.host.most_at_once.load(Ordering::SeqCst), 2);
    let all = steps_of(&b, &done).into_iter().find(|r| r.id == "all").unwrap();
    assert_eq!(all.output, json!(["late 1", "late 2", "late 3", "late 4", "late 5", "late 6"]), "outputs stay in item order");
}

#[tokio::test]
async fn steps_that_need_nothing_run_together() {
    let b = bench(slow(300));
    let started = Instant::now();
    let done = run(
        &b,
        flow(json!([
            { "id": "a", "tool": "mcp:x/slow", "with": { "n": "a" } },
            { "id": "b", "needs": [], "tool": "mcp:x/slow", "with": { "n": "b" } },
            { "id": "c", "needs": ["a", "b"], "set": { "both": ["${{ steps.a.output }}", "${{ steps.b.output }}"] } }
        ])),
    )
    .await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(b.host.most_at_once.load(Ordering::SeqCst), 2, "a and b did not run together");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(steps_of(&b, &done).into_iter().find(|r| r.id == "c").unwrap().output, json!({ "both": ["late a", "late b"] }));
}

#[tokio::test]
async fn repeat_until_and_its_limit() {
    let b = bench(TestHost::default());
    let f = flow(json!([
        { "id": "count", "repeat": [{ "id": "n", "set": { "n": { "expr": { "add": [{ "var": "round" }, 1] } } } }],
          "until": { "gte": [{ "var": "steps.n.output.n" }, 3] }, "max": 10 },
        { "set": { "rounds": "${{ steps.count.rounds }}", "last": "${{ steps.count.output.n }}" } }
    ]));
    let done = run(&b, f).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    let last = steps_of(&b, &done).into_iter().last().unwrap();
    assert_eq!(last.output, json!({ "rounds": 3, "last": 3 }));
    let never = flow(json!([{ "repeat": [{ "set": { "x": 1 } }], "until": { "eq": [1, 2] }, "max": 4 }]));
    let done = run(&b, never.clone()).await;
    assert_eq!(done.error.unwrap().kind, "limit");
    let mut lenient = never;
    lenient["steps"][0]["on_max"] = json!("continue");
    assert_eq!(run(&b, lenient).await.status, RunStatus::Succeeded);
}

#[tokio::test]
async fn waiting_does_not_count_toward_the_run_timeout() {
    let b = bench(TestHost::default());
    let f = json!({ "flow": "cluaiz.flow/1", "name": "test", "limits": { "timeout": "200ms" }, "steps": [{ "wait": { "for": "500ms" } }, { "set": { "after": true } }] });
    let done = run(&b, f).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert!(done.running_ms < 200, "running time {} counts the wait", done.running_ms);
    let busy = json!({ "flow": "cluaiz.flow/1", "name": "test", "limits": { "timeout": "200ms" }, "steps": [{ "tool": "mcp:x/slow", "timeout": "5s" }] });
    let b = bench(slow(800));
    assert_eq!(run(&b, busy).await.error.unwrap().kind, "limit");
    let too_long = json!({ "flow": "cluaiz.flow/1", "name": "test", "limits": { "max_wait": "1s" }, "steps": [{ "wait": { "for": "1h" } }] });
    assert_eq!(run(&b, too_long).await.error.unwrap().kind, "limit");
}

#[tokio::test]
async fn wait_for_an_event_sent_by_a_branch() {
    let b = bench(TestHost::default());
    let f = flow(json!([{ "id": "both", "parallel": {
        "listen": [{ "id": "got", "wait": { "event": "order.paid", "filter": { "eq": [{ "var": "event.data.id" }, 7] } }, "timeout": "5s" }],
        "send": [{ "wait": { "for": "100ms" } }, { "emit": "order.paid", "with": { "id": 6 } }, { "emit": "order.paid", "with": { "id": 7 } }] } }]));
    let done = run(&b, f).await;
    // The run's own events do not reach it: the wait times out
    assert_eq!(done.error.as_ref().map(|e| e.kind.as_str()), Some("timeout"));
    // An event from another run does
    let b = bench(TestHost::default());
    let bus = b.host.bus.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = bus.send(cluaiz_automation::host::HostEvent { name: "order.paid".into(), data: json!({ "id": 7 }), from: json!({ "flow": "shop", "run_id": "run_other" }) });
    });
    let done = run(&b, flow(json!([{ "id": "got", "wait": { "event": "order.paid" }, "timeout": "5s" }]))).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(steps_of(&b, &done)[0].output["data"], json!({ "id": 7 }));
    let quiet = run(&b, flow(json!([{ "wait": { "event": "never" }, "timeout": "50ms", "on_timeout": "continue" }, { "set": { "went_on": true } }]))).await;
    assert_eq!(quiet.status, RunStatus::Succeeded);
}

#[tokio::test]
async fn wait_for_the_runs_own_webhook() {
    let b = bench(TestHost::default());
    let done = run(&b, flow(json!([{ "id": "link", "set": { "url": "${{ run.webhook_url }}" } }, { "id": "paid", "wait": { "webhook": true }, "timeout": "1h" }]))).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    let records = steps_of(&b, &done);
    let token = b.host.tokens_seen.lock().unwrap()[0].clone();
    assert!(token.len() >= 32);
    assert_eq!(records[0].output["url"], json!(format!("http://test/v1/hooks/runs/{}/{token}", done.run_id)));
    assert_eq!(records[1].output["body"], json!({ "paid": true }));
}

#[tokio::test]
async fn approve_yes_no_and_no_answer() {
    let b = bench(TestHost::default());
    b.host.owner_says.store(true, Ordering::SeqCst);
    let ask = flow(json!([{ "id": "ok", "approve": "Send ${{ inputs.n }} mails?", "with": { "to": "all" }, "timeout": "1h" }, { "tool": "mcp:x/send" }]));
    let done = run_with(&b, request(json!({ "flow": "cluaiz.flow/1", "name": "test", "inputs": { "n": { "type": "integer", "default": 3 } }, "steps": ask["steps"] }), json!({}))).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    let question = b.host.questions.lock().unwrap()[0].clone();
    assert_eq!((question.question.as_str(), question.details.clone()), ("Send 3 mails?", json!({ "to": "all" })));
    assert!(question.timeout_ms.unwrap() > 3_500_000, "the step's timeout reaches the owner's question");
    b.host.owner_says.store(false, Ordering::SeqCst);
    let done = run(&b, ask.clone()).await;
    assert_eq!(done.error.unwrap().kind, "approval_rejected");
    assert_eq!(b.host.calls_to("send"), 1, "nothing is sent after a no");
    let mut go_on = ask.clone();
    go_on["steps"][0]["on_reject"] = json!("continue");
    let done = run(&b, go_on).await;
    assert_eq!(steps_of(&b, &done)[0].output["approved"], json!(false));
    b.host.owner_silent.store(true, Ordering::SeqCst);
    let mut silent = ask;
    silent["steps"][0]["timeout"] = json!("50ms");
    silent["steps"][0]["on_timeout"] = json!("approve");
    let done = run(&b, silent).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(steps_of(&b, &done)[0].output["by"], json!("timeout"));
}

#[tokio::test]
async fn emit_run_and_code() {
    let b = bench(TestHost::default());
    let done = run(
        &b,
        flow(json!([
            { "id": "sent", "emit": "order.saved", "with": { "id": 42 } },
            { "id": "child", "run": "invoice", "with": { "order": 42 } },
            { "id": "later", "run": "archive", "wait": false },
            { "id": "math", "code": "score.wasm", "with": { "n": 21 } }
        ])),
    )
    .await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    let out: Vec<Value> = steps_of(&b, &done).into_iter().map(|r| r.output).collect();
    assert_eq!(out[0], json!({ "name": "order.saved", "delivered": 1 }));
    assert_eq!(out[1], json!({ "got": { "order": 42 } }));
    assert_eq!(out[2], json!({ "run_id": "run_child" }));
    assert_eq!(out[3], json!({ "doubled": 42, "network": ["api.example.com"] }));
    let parent = b.host.flows_run.lock().unwrap()[0].parent.clone();
    assert_eq!((parent["run_id"].clone(), parent["depth"].clone()), (json!(done.run_id), json!(1)));
    assert_eq!(run(&b, flow(json!([{ "run": "broken" }]))).await.error.unwrap().kind, "tool");
    assert_eq!(run(&b, flow(json!([{ "code": "x.wasm", "fn": "fails" }]))).await.error.unwrap().kind, "code");
    assert_eq!(run(&b, flow(json!([{ "code": "x.wasm", "fn": "loops" }]))).await.error.unwrap().kind, "limit");
    assert_eq!(run(&b, flow(json!([{ "emit": "engine.started" }]))).await.error.unwrap().kind, "invalid_input");
    // Too deep: this run was itself started eight levels down
    let mut deep = request(flow(json!([{ "run": "again" }])), json!({}));
    deep.trigger = json!({ "kind": "run", "parent": { "depth": 8 } });
    assert_eq!(run_with(&b, deep).await.error.unwrap().kind, "limit");
}

#[tokio::test]
async fn an_agent_calls_tools_then_answers() {
    let b = bench(TestHost::default());
    *b.host.agent_script.lock().unwrap() = vec![
        AskAnswer { tool_calls: vec![json!({ "id": "c1", "type": "function", "function": { "name": "mcp_x_weather", "arguments": "{\"city\":\"Pune\"}" } })], ..Default::default() },
        AskAnswer { tool_calls: vec![json!({ "id": "c2", "type": "function", "function": { "name": "nothing_like_this", "arguments": "{}" } })], ..Default::default() },
        AskAnswer { text: "{\"city\": \"Pune\", \"hot\": true}".into(), ..Default::default() },
    ];
    let f = flow(json!([{ "id": "look", "agent": "Is it hot in Pune?", "tools": ["mcp:x/weather"], "max_turns": 5,
        "output_schema": { "type": "object", "properties": { "hot": { "type": "boolean" } }, "required": ["hot"] } }]));
    let done = run(&b, f).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    let record = steps_of(&b, &done).remove(0);
    assert_eq!(record.output, json!({ "city": "Pune", "hot": true }));
    assert_eq!(record.detail["rounds"].as_array().unwrap().len(), 2);
    assert_eq!(record.detail["rounds"][1]["calls"][0]["ok"], json!(false), "an unknown tool is told to the model, not a failure");
    assert_eq!(b.host.calls_to("weather"), 1);
    let asks = b.host.asks.lock().unwrap();
    assert_eq!(asks[0].tools[0]["function"]["name"], json!("mcp_x_weather"));
    let tool_message = asks[1].messages.iter().find(|m| m["role"] == "tool").unwrap();
    assert!(tool_message["content"].as_str().unwrap().contains("31"));
}

#[tokio::test]
async fn an_agent_at_max_turns_is_asked_for_its_answer() {
    let b = bench(TestHost::default());
    let call = || AskAnswer { tool_calls: vec![json!({ "id": "c", "function": { "name": "mcp_x_echo", "arguments": "{}" } })], ..Default::default() };
    *b.host.agent_script.lock().unwrap() = vec![call(), call(), AskAnswer { text: "best guess".into(), ..Default::default() }];
    let done = run(&b, flow(json!([{ "agent": "Keep looking", "tools": ["mcp:x/echo"], "max_turns": 2 }]))).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(steps_of(&b, &done)[0].output, json!("best guess"));
    let asks = b.host.asks.lock().unwrap();
    assert!(asks.last().unwrap().tools.is_empty(), "the last turn has no tools");
}

#[tokio::test]
async fn outside_content_with_sending_tools_waits_for_the_owner() {
    let b = bench(TestHost::default());
    let f = json!({ "flow": "cluaiz.flow/1", "name": "test", "secrets": ["H"], "on": [{ "webhook": { "secret": "env:H" } }],
        "steps": [{ "agent": "Answer ${{ trigger.body }}", "tools": ["mcp:x/echo", "mcp:x/send"] }] });
    let done = run(&b, f.clone()).await;
    assert_eq!(done.error.unwrap().kind, "approval_rejected");
    assert_eq!(b.host.owner_asked.load(Ordering::SeqCst), 1);
    let report = cluaiz_automation::validate::validate(&f, Some("test"), &TestCatalog);
    assert!(report.problems.iter().any(|p| p.code == "untrusted_with_tools"), "{:?}", report.problems);
    let mut allowed = f;
    allowed["steps"][0]["allow_untrusted"] = json!(true);
    assert_eq!(run(&b, allowed).await.status, RunStatus::Succeeded);
}

#[tokio::test]
async fn a_wait_goes_on_after_a_restart_with_the_time_that_was_left() {
    let b = bench(TestHost::default());
    let f = flow(json!([{ "id": "first", "tool": "mcp:x/echo" }, { "id": "pause", "wait": { "for": "1s" } }, { "id": "last", "set": { "done": true } }]));
    let runner = Runner::new(b.host.clone(), b.paths.clone());
    let created = runner.create(request(f, json!({}))).unwrap();
    let run_id = created.run_id().to_string();
    let stopped = tokio::time::timeout(Duration::from_millis(400), runner.execute(created, Arc::new(AtomicBool::new(false)), no_events())).await;
    assert!(stopped.is_err());
    let dir = b.paths.run_dir("test", &run_id);
    assert_eq!(journal::read_run(&dir).unwrap().status, RunStatus::Waiting);
    let reopened = runner.reopen(dir, serde_json::from_str(ALL).unwrap()).unwrap();
    let started = Instant::now();
    let done = runner.execute(reopened, Arc::new(AtomicBool::new(false)), no_events()).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert!(started.elapsed() < Duration::from_millis(900), "the wait started again from zero: {:?}", started.elapsed());
    assert_eq!(b.host.calls_to("echo"), 1);
    assert_eq!(done.steps_done, 3, "each step counted once across the restart");
}

#[tokio::test]
async fn dry_runs_send_nothing() {
    let b = bench(TestHost::default());
    let mut dry = request(flow(json!([{ "approve": "Go?" }, { "emit": "x.y" }, { "code": "m.wasm", "with": { "n": 1 } }])), json!({}));
    dry.dry_run = true;
    let done = run_with(&b, dry).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(b.host.owner_asked.load(Ordering::SeqCst), 0);
    assert!(b.host.emitted.lock().unwrap().is_empty());
}

#[test]
fn the_new_steps_validate() {
    let f = json!({ "flow": "cluaiz.flow/1", "name": "test", "steps": [
        { "id": "count", "repeat": [{ "id": "n", "set": { "before": "${{ steps.count.previous }}" } }], "until": { "eq": [{ "var": "round" }, 2] }, "max": 5 },
        { "id": "got", "wait": { "event": "order.paid", "filter": { "eq": [{ "var": "event.data.id" }, 7] } }, "timeout": "1h" },
        { "id": "look", "agent": "Look", "tools": ["mcp:x/echo"], "stop_when": { "gte": [{ "var": "round.index" }, 2] } },
        { "id": "p", "parallel": { "a": [{ "set": { "x": "${{ steps.look.output }}" } }], "b": [{ "approve": "ok?" }] } },
        { "emit": "done.now", "with": "${{ steps.p.output }}" },
        { "run": "other", "with": { "x": 1 } },
        { "code": "m.wasm", "fn": "main", "memory": "64MB", "fuel": 1000 }
    ] });
    let report = cluaiz_automation::validate::validate(&f, Some("test"), &cluaiz_automation::host::NoCatalog);
    let errors: Vec<_> = report.problems.iter().filter(|p| p.is_error()).collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[tokio::test]
async fn secret_values_never_reach_the_records() {
    std::env::set_var("POWER_TEST_SECRET", "s3cret-value-xyz");
    let b = bench(TestHost::default());
    let f = json!({ "flow": "cluaiz.flow/1", "name": "test", "secrets": ["POWER_TEST_SECRET"],
        "steps": [{ "id": "leak", "tool": "mcp:x/echo", "with": { "token": "s3cret-value-xyz" } }],
        "outputs": { "said": "${{ steps.leak.output }}" } });
    let done = run(&b, f).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    let dir = b.paths.run_dir("test", &done.run_id);
    let files = std::fs::read_to_string(dir.join("journal.jsonl")).unwrap() + &std::fs::read_to_string(dir.join("run.json")).unwrap();
    assert!(!files.contains("s3cret-value-xyz") && files.contains("***"));
    assert!(!done.outputs.to_string().contains("s3cret-value-xyz"));
}

#[tokio::test]
async fn rerun_from_a_step_reuses_what_came_before() {
    let b = bench(TestHost::default());
    let f = flow(json!([
        { "id": "first", "tool": "mcp:x/echo", "with": { "n": 1 } },
        { "id": "keep", "set": { "kept": "${{ steps.first.output }}" } },
        { "id": "send", "tool": "mcp:x/broken" },
        { "id": "after", "set": { "done": true } }
    ]));
    let failed = run(&b, f.clone()).await;
    assert_eq!(failed.status, RunStatus::Failed);
    let runner = Runner::new(b.host.clone(), b.paths.clone());
    let mut fixed = f;
    fixed["steps"][2]["tool"] = json!("mcp:x/send");
    let mut again = request(fixed, json!({}));
    again.rerun_of = json!(failed.run_id);
    let created = runner.rerun(again, &b.paths.run_dir("test", &failed.run_id), "send").unwrap();
    let done = runner.execute(created, Arc::new(AtomicBool::new(false)), no_events()).await;
    assert_eq!(done.status, RunStatus::Succeeded, "{:?}", done.error);
    assert_eq!(b.host.calls_to("echo"), 1, "steps before `send` are not run again");
    let statuses: Vec<String> = steps_of(&b, &done).into_iter().map(|r| format!("{}:{}", r.id, r.status)).collect();
    assert_eq!(statuses, vec!["first:reused", "keep:reused", "send:succeeded", "after:succeeded"]);
    assert!(runner.rerun(request(flow(json!([{ "set": {} }])), json!({})), &b.paths.run_dir("test", &failed.run_id), "nope").is_err());
}

#[tokio::test]
async fn a_chain_of_events_stops() {
    let b = bench(TestHost::default());
    let mut chained = request(flow(json!([{ "emit": "ping" }])), json!({}));
    chained.trigger = json!({ "kind": "event", "name": "ping", "hops": 8 });
    assert_eq!(run_with(&b, chained).await.error.unwrap().kind, "limit");
    let mut short = request(flow(json!([{ "emit": "ping" }])), json!({}));
    short.trigger = json!({ "kind": "event", "name": "ping", "hops": 2 });
    assert_eq!(run_with(&b, short).await.status, RunStatus::Succeeded);
}
