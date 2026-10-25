//! The protocol end to end through `Engine::call`, with a small test registry

use cluaiz_cep::error::*;
use cluaiz_cep::*;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // fields exist for the params schema and checks
struct Write {
    path: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    nested: Option<Nested>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Nested {
    size: u32,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Copy {
    from: String,
    to: String,
}

#[derive(Default)]
struct Log(Mutex<Vec<(String, String, Option<i32>)>>);

impl Audit for Log {
    fn record(&self, e: &AuditEntry) {
        self.0.lock().unwrap().push((e.principal.into(), e.method.into(), e.code));
    }
}

fn engine(log: Arc<Log>) -> Engine<()> {
    let mut registry: Registry<()> = Registry::new();
    registry
        .add(
            "fs.write",
            Meta::new(Scope::Write, "Writes a file").example(json!({"path": "a.txt", "text": "hi"}), json!({"bytes": 2})),
            |_: (), call: Call, p: Write| async move {
                call.check_path(&PathBuf::from(&p.path))?;
                Ok(json!({ "bytes": p.text.unwrap_or_default().len() }))
            },
        )
        .add(
            "fs.copy",
            Meta::new(Scope::Write, "Two paths, one approval").example(json!({"from": "a", "to": "b"}), json!({"copied": true})),
            |_: (), call: Call, p: Copy| async move {
                call.check_path(&PathBuf::from(&p.from))?;
                call.check_path(&PathBuf::from(&p.to))?;
                Ok(json!({ "copied": true }))
            },
        )
        .add(
            "system.info",
            Meta::new(Scope::Read, "Version").example(json!({}), json!({"version": "0.1.0"})),
            |_: (), _: Call, _: NoParams| async move { Ok(json!({ "version": "0.1.0" })) },
        )
        .add(
            "system.panic",
            Meta::new(Scope::Read, "Panics").example(json!({}), json!(0)),
            |_: (), _: Call, _: NoParams| async move {
                if true {
                    panic!("boom");
                }
                Ok(0)
            },
        )
        .add(
            "ingest.run",
            Meta::new(Scope::Read, "Slow work as a task").task().example(json!({}), json!({"task": {}})),
            |_: (), call: Call, _: NoParams| async move {
                call.start_task(|handle| async move {
                    handle.progress(Progress { done: 1, total: Some(1), ..Default::default() });
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Ok(json!({ "chunks": 3 }))
                })
            },
        );
    Engine::new(registry, ApprovalStore::new(Duration::from_secs(300)), TaskStore::new(8, 60_000, 120_000), Limits::default(), "Cluaiz Engine Protocol", "0.1.0")
        .with_audit(log)
}

fn token(scopes: &[Scope], on_denied: OnDenied) -> Arc<Principal> {
    Arc::new(Principal {
        id: "extension:e".into(),
        scopes: scopes.to_vec(),
        roots: Some(vec![PathBuf::from("/work")]),
        on_denied,
        owner: false,
    })
}

async fn call(engine: &Engine<()>, who: &Arc<Principal>, body: Value) -> Option<Value> {
    let reply = engine.call((), who.clone(), body.to_string().as_bytes()).await?;
    Some(serde_json::from_slice(&reply).unwrap())
}

#[tokio::test]
async fn test_single_notification_and_batch() {
    let engine = engine(Arc::default());
    let owner = Arc::new(Principal::owner());
    let reply = call(&engine, &owner, json!({"jsonrpc": "2.0", "id": 1, "method": "system.info"})).await.unwrap();
    assert_eq!(reply, json!({"jsonrpc": "2.0", "id": 1, "result": {"version": "0.1.0"}}));
    assert_eq!(call(&engine, &owner, json!({"jsonrpc": "2.0", "method": "system.info"})).await, None);
    let batch = call(&engine, &owner, json!([
        {"jsonrpc": "2.0", "id": "a", "method": "system.info"},
        {"jsonrpc": "2.0", "method": "system.info"},
        {"jsonrpc": "2.0", "id": "b", "method": "system.infoo"},
        1
    ]))
    .await
    .unwrap();
    let ids: Vec<&Value> = batch.as_array().unwrap().iter().map(|r| &r["id"]).collect();
    assert_eq!(ids, vec![&json!("a"), &json!("b"), &Value::Null]);
    assert_eq!(batch[1]["error"]["code"], METHOD_NOT_FOUND);
    assert_eq!(batch[1]["error"]["data"]["did_you_mean"], json!(["system.info"]));
    let only_notes = call(&engine, &owner, json!([{"jsonrpc": "2.0", "method": "system.info"}])).await;
    assert_eq!(only_notes, None);
    let parse = engine.call((), owner.clone(), b"{not json").await.unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&parse).unwrap()["error"]["code"], PARSE_ERROR);
}

#[tokio::test]
async fn test_params_errors_show_the_path_and_example() {
    let engine = engine(Arc::default());
    let owner = Arc::new(Principal::owner());
    let reply = call(&engine, &owner, json!({"jsonrpc": "2.0", "id": 1, "method": "fs.write", "params": {"path": "/work/a", "nested": {"size": "big"}}})).await.unwrap();
    let error = &reply["error"];
    assert_eq!(error["code"], INVALID_PARAMS);
    assert_eq!(error["data"]["errors"][0]["path"], "nested.size");
    assert_eq!(error["data"]["example"], json!({"path": "a.txt", "text": "hi"}));
    // `caller: "user"` is not a field any more: it is rejected, not trusted
    let reply = call(&engine, &owner, json!({"jsonrpc": "2.0", "id": 2, "method": "fs.write", "params": {"path": "/work/a", "caller": "user"}})).await.unwrap();
    assert_eq!(reply["error"]["code"], INVALID_PARAMS);
}

#[tokio::test]
async fn test_scope_roots_and_approval_round_trip() {
    let log = Arc::new(Log::default());
    let engine = engine(log.clone());
    let reader = token(&[Scope::Read], OnDenied::Error);
    let reply = call(&engine, &reader, json!({"jsonrpc": "2.0", "id": 1, "method": "fs.write", "params": {"path": "/work/a"}})).await.unwrap();
    assert_eq!((reply["error"]["code"].as_i64(), reply["error"]["data"]["needs"].as_str()), (Some(FORBIDDEN as i64), Some("write")));
    // Discovery hides what the caller may not call
    let doc = call(&engine, &reader, json!({"jsonrpc": "2.0", "id": 2, "method": "rpc.discover"})).await.unwrap();
    let names: Vec<&str> = doc["result"]["methods"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"system.info") && !names.contains(&"fs.write"));

    let asker = token(&[Scope::Read, Scope::Write], OnDenied::Ask);
    let inside = call(&engine, &asker, json!({"jsonrpc": "2.0", "id": 3, "method": "fs.write", "params": {"path": "/work/a", "text": "hi"}})).await.unwrap();
    assert_eq!(inside["result"]["bytes"], 2);
    let write_outside = json!({"jsonrpc": "2.0", "id": 4, "method": "fs.write", "params": {"path": "/other/x", "text": "hi"}});
    let asked = call(&engine, &asker, write_outside.clone()).await.unwrap();
    assert_eq!(asked["error"]["code"], APPROVAL_REQUIRED);
    let approval = asked["error"]["data"]["approval_id"].as_str().unwrap().to_string();
    engine.shared().approvals.decide(&approval, true, None).unwrap();
    let mut retry = write_outside.clone();
    retry["params"]["_meta"] = json!({"approval": approval});
    assert_eq!(call(&engine, &asker, retry.clone()).await.unwrap()["result"]["bytes"], 2);
    // Used once
    assert_eq!(call(&engine, &asker, retry.clone()).await.unwrap()["error"]["code"], APPROVAL_REQUIRED);
    // Other params with an approval id: no
    let mut other = retry;
    other["params"]["path"] = json!("/other/y");
    assert_eq!(call(&engine, &asker, other).await.unwrap()["error"]["code"], APPROVAL_REQUIRED);

    let lines = log.0.lock().unwrap();
    assert!(lines.iter().any(|(_, m, c)| m == "fs.write" && *c == Some(FORBIDDEN)));
    assert!(lines.iter().any(|(_, m, c)| m == "fs.write" && c.is_none()));
    assert!(!lines.iter().any(|(_, m, _)| m == "system.info"), "reads are not audited");
}

#[tokio::test]
async fn test_tasks_panics_and_discovery_document() {
    let engine = engine(Arc::default());
    let owner = Arc::new(Principal::owner());
    let started = call(&engine, &owner, json!({"jsonrpc": "2.0", "id": 1, "method": "ingest.run"})).await.unwrap();
    let task_id = started["result"]["task"]["task_id"].as_str().unwrap().to_string();
    let result = call(&engine, &owner, json!({"jsonrpc": "2.0", "id": 2, "method": "tasks.result", "params": {"task_id": task_id, "wait_ms": 5000}})).await.unwrap();
    assert_eq!(result["result"], json!({"chunks": 3}));
    let other = token(&Scope::ALL, OnDenied::Error);
    let hidden = call(&engine, &other, json!({"jsonrpc": "2.0", "id": 3, "method": "tasks.get", "params": {"task_id": task_id}})).await.unwrap();
    assert_eq!(hidden["error"]["code"], NOT_FOUND);

    let panicked = call(&engine, &owner, json!({"jsonrpc": "2.0", "id": 4, "method": "system.panic"})).await.unwrap();
    assert_eq!(panicked["error"]["code"], INTERNAL_ERROR);

    let doc = call(&engine, &owner, json!({"jsonrpc": "2.0", "id": 5, "method": "rpc.discover", "params": {"group": "fs"}})).await.unwrap()["result"].clone();
    assert_eq!(doc["openrpc"], "1.3.2");
    assert_eq!(doc["methods"].as_array().unwrap().len(), 2, "fs.copy and fs.write");
    let method = doc["methods"].as_array().unwrap().iter().find(|m| m["name"] == "fs.write").unwrap();
    assert_eq!((method["name"].as_str(), method["x-cep"]["scope"].as_str()), (Some("fs.write"), Some("write")));
    let params: Vec<(&str, bool)> = method["params"].as_array().unwrap().iter().map(|p| (p["name"].as_str().unwrap(), p["required"].as_bool().unwrap())).collect();
    assert!(params.contains(&("path", true)) && params.contains(&("text", false)));
    // Every $ref points into components.schemas, and is there
    let text = doc.to_string();
    for target in text.split("\"$ref\":\"").skip(1).map(|rest| &rest[..rest.find('"').unwrap()]) {
        let name = target.strip_prefix("#/components/schemas/").expect(target);
        assert!(doc["components"]["schemas"].get(name).is_some(), "{name}");
    }
}

#[tokio::test]
async fn test_one_approval_covers_every_path_of_the_call() {
    let engine = engine(Arc::default());
    let asker = token(&[Scope::Read, Scope::Write], OnDenied::Ask);
    let copy = json!({"jsonrpc": "2.0", "id": 1, "method": "fs.copy", "params": {"from": "/other/a", "to": "/elsewhere/b"}});
    let asked = call(&engine, &asker, copy.clone()).await.unwrap();
    let approval = asked["error"]["data"]["approval_id"].as_str().unwrap().to_string();
    engine.shared().approvals.decide(&approval, true, None).unwrap();
    let mut retry = copy;
    retry["params"]["_meta"] = json!({"approval": approval});
    // Both outside paths pass on the one approval; it is still used up for any later call
    assert_eq!(call(&engine, &asker, retry.clone()).await.unwrap()["result"]["copied"], true);
    assert_eq!(call(&engine, &asker, retry).await.unwrap()["error"]["code"], APPROVAL_REQUIRED);
}
