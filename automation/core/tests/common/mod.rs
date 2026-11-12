//! A test host that stands in for the Engine: it counts what each step asked for, so the tests
//! check behaviour (what ran, how often, with what), not just parsing.
#![allow(dead_code)]

use async_trait::async_trait;
use cluaiz_automation::host::{AskAnswer, AskRequest, Catalog, CodeCall, FlowCall, HostEvent, NodeInfo, OwnerAnswer, OwnerQuestion, RunInfo, StepHost};
use cluaiz_automation::journal::{self, Paths, RunRecord};
use cluaiz_automation::run::{Events, RunRequest, Runner};
use cluaiz_automation::{FlowError, Kind};
use serde_json::{json, Map, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct TestHost {
    pub calls: Mutex<Vec<(String, Value)>>,
    pub asks: Mutex<Vec<AskRequest>>,
    pub flaky_left: AtomicUsize,
    pub owner_says: AtomicBool,
    /// The owner does not answer at all
    pub owner_silent: AtomicBool,
    pub owner_asked: AtomicUsize,
    pub questions: Mutex<Vec<OwnerQuestion>>,
    /// `mcp:x/slow` waits this long
    pub slow_ms: u64,
    /// Tools running now, and the most at once
    pub running: AtomicUsize,
    pub most_at_once: AtomicUsize,
    pub bus: tokio::sync::broadcast::Sender<HostEvent>,
    pub emitted: Mutex<Vec<(String, Value)>>,
    pub flows_run: Mutex<Vec<FlowCall>>,
    pub hook_requests: Mutex<Vec<Value>>,
    pub tokens_seen: Mutex<Vec<String>>,
    /// What the agent's model does, turn by turn: tool calls, then an answer
    pub agent_script: Mutex<Vec<AskAnswer>>,
}

impl Default for TestHost {
    fn default() -> Self {
        Self {
            calls: Mutex::default(),
            asks: Mutex::default(),
            flaky_left: AtomicUsize::new(0),
            owner_says: AtomicBool::new(false),
            owner_silent: AtomicBool::new(false),
            owner_asked: AtomicUsize::new(0),
            questions: Mutex::default(),
            slow_ms: 0,
            running: AtomicUsize::new(0),
            most_at_once: AtomicUsize::new(0),
            bus: tokio::sync::broadcast::channel(64).0,
            emitted: Mutex::default(),
            flows_run: Mutex::default(),
            hook_requests: Mutex::default(),
            tokens_seen: Mutex::default(),
            agent_script: Mutex::default(),
        }
    }
}

impl TestHost {
    pub fn calls_to(&self, name: &str) -> usize {
        self.calls.lock().unwrap().iter().filter(|(m, p)| m == name || p.get("fn").and_then(Value::as_str) == Some(name)).count()
    }
}

pub struct TestCatalog;
impl Catalog for TestCatalog {
    fn tool(&self, name: &str) -> Option<NodeInfo> {
        Some(NodeInfo { read_only: name == "mcp:x/echo", network: name.starts_with("mcp:web/"), sends: name == "mcp:x/send", ..Default::default() })
    }
    fn method(&self, name: &str) -> Option<NodeInfo> {
        Some(NodeInfo { read_only: name == "fs.read", scope: Some(if name == "fs.read" { "read" } else { "write" }.into()), ..Default::default() })
    }
    fn names(&self) -> Vec<String> {
        Vec::new()
    }
    fn model_installed(&self, _: &str) -> Option<bool> {
        None
    }
}

#[async_trait]
impl StepHost for TestHost {
    async fn call(&self, _run: &RunInfo, method: &str, params: Value) -> Result<Value, FlowError> {
        self.calls.lock().unwrap().push((method.to_string(), params.clone()));
        match (method, params.get("fn").and_then(Value::as_str)) {
            ("fs.read", _) => Ok(json!({ "text": format!("contents of {}", params["path"].as_str().unwrap_or_default()) })),
            ("notes.create", _) => Ok(json!({ "id": "note_1", "text": params["text"] })),
            ("tools.call", Some("echo")) => Ok(json!({ "content": [{ "type": "text", "text": params["input"].to_string() }] })),
            ("tools.call", Some("flaky")) => {
                if self.flaky_left.load(Ordering::SeqCst) > 0 {
                    self.flaky_left.fetch_sub(1, Ordering::SeqCst);
                    return Err(FlowError::new(Kind::Network, "the server did not answer"));
                }
                Ok(json!("ok"))
            }
            ("tools.call", Some("big")) => Ok(Value::Array((0..50_000).map(|i| json!({ "n": i, "text": "some words to make it large" })).collect())),
            ("tools.call", Some("slow")) => {
                let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
                self.most_at_once.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(self.slow_ms)).await;
                self.running.fetch_sub(1, Ordering::SeqCst);
                let n = match &params["input"]["n"] {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                Ok(json!(format!("late {n}")))
            }
            ("tools.call", Some("send")) => Ok(json!({ "sent": true })),
            ("tools.call", Some("broken")) => Err(FlowError::new(Kind::Tool, "no such repo")),
            ("tools.call", Some("weather")) => Ok(json!({ "city": params["input"]["city"], "temp": 31 })),
            _ => Ok(Value::Null),
        }
    }

    async fn ask(&self, _run: &RunInfo, request: AskRequest) -> Result<AskAnswer, FlowError> {
        let attempt = {
            let mut asks = self.asks.lock().unwrap();
            asks.push(request.clone());
            asks.len()
        };
        if !request.messages.is_empty() {
            let mut script = self.agent_script.lock().unwrap();
            let next = if script.is_empty() { AskAnswer { text: "done".into(), ..Default::default() } } else { script.remove(0) };
            return Ok(AskAnswer { model: "test-model".into(), prompt_tokens: 3, completion_tokens: 2, ..next });
        }
        let text = match &request.output_schema {
            // The first answer does not match the schema; the second does
            Some(_) if attempt % 2 == 1 => "{\"kind\": \"maybe\"}".to_string(),
            Some(_) => "```json\n{\"kind\": \"bug\"}\n```".to_string(),
            None => format!("  summary of: {}  ", request.user.lines().next().unwrap_or_default()),
        };
        Ok(AskAnswer { text, model: request.model.clone().unwrap_or("test-model".into()), prompt_tokens: 10, completion_tokens: 5, ..Default::default() })
    }

    async fn approval(&self, _run: &RunInfo, question: OwnerQuestion) -> Result<OwnerAnswer, FlowError> {
        self.owner_asked.fetch_add(1, Ordering::SeqCst);
        self.questions.lock().unwrap().push(question.clone());
        if self.owner_silent.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(question.timeout_ms.unwrap_or(0))).await;
            return Ok(OwnerAnswer::default());
        }
        Ok(OwnerAnswer { approved: Some(self.owner_says.load(Ordering::SeqCst)), note: Some("from the test".into()) })
    }

    fn catalog(&self) -> &dyn Catalog {
        &TestCatalog
    }

    async fn emit(&self, run: &RunInfo, name: &str, data: Value, _hops: u64) -> Result<u64, FlowError> {
        self.emitted.lock().unwrap().push((name.to_string(), data.clone()));
        let _ = self.bus.send(HostEvent { name: name.to_string(), data, from: json!({ "flow": run.flow, "run_id": run.run_id }) });
        Ok(1)
    }

    fn events(&self) -> Option<tokio::sync::broadcast::Receiver<HostEvent>> {
        Some(self.bus.subscribe())
    }

    async fn wait_webhook(&self, _run: &RunInfo, token: &str, timeout_ms: u64) -> Result<Option<Value>, FlowError> {
        self.tokens_seen.lock().unwrap().push(token.to_string());
        // A request arrives soon unless the test wants a timeout
        if timeout_ms < 100 {
            tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
            return Ok(None);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        let request = json!({ "method": "POST", "body": { "paid": true } });
        self.hook_requests.lock().unwrap().push(request.clone());
        Ok(Some(request))
    }

    fn webhook_url(&self, run_id: &str, token: &str) -> Option<String> {
        Some(format!("http://test/v1/hooks/runs/{run_id}/{token}"))
    }

    async fn run_flow(&self, _run: &RunInfo, call: FlowCall) -> Result<Value, FlowError> {
        self.flows_run.lock().unwrap().push(call.clone());
        match call.name.as_str() {
            "broken" => Err(FlowError::new(Kind::Tool, "the other flow failed")),
            _ if !call.wait => Ok(json!({ "run_id": "run_child" })),
            _ => Ok(json!({ "got": call.inputs })),
        }
    }

    async fn code(&self, _run: &RunInfo, call: CodeCall) -> Result<Value, FlowError> {
        match call.function.as_str() {
            "fails" => Ok(json!({ "error": { "message": "bad lead" } })),
            "loops" => Err(FlowError::new(Kind::Limit, "the module ran out of fuel")),
            _ => Ok(json!({ "doubled": call.input["n"].as_i64().unwrap_or(0) * 2, "network": call.network })),
        }
    }
}

pub struct Bench {
    pub _dir: tempfile::TempDir,
    pub paths: Paths,
    pub host: Arc<TestHost>,
}

pub fn bench(host: TestHost) -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths { flows: dir.path().join("flows") };
    Bench { _dir: dir, paths, host: Arc::new(host) }
}

pub const ALL: &str = r#"{ "tools": ["*"], "methods": ["*"], "models": ["*"], "scopes": ["read", "write", "exec"], "network": ["api.example.com"] }"#;

pub fn request(flow: Value, inputs: Value) -> RunRequest {
    RunRequest {
        flow,
        scope: "user".into(),
        permissions: serde_json::from_str(ALL).unwrap(),
        inputs,
        trigger: json!({ "kind": "manual", "by": "test" }),
        dry_run: false,
        mocks: Map::new(),
        run_id: journal::new_run_id(),
        task_id: None,
        parent: Value::Null,
        rerun_of: Value::Null,
        flow_dir: None,
    }
}

pub fn no_events() -> Events {
    Arc::new(|_| {})
}

pub async fn run_with(b: &Bench, request: RunRequest) -> RunRecord {
    let runner = Runner::new(b.host.clone(), b.paths.clone());
    let created = runner.create(request).unwrap();
    runner.execute(created, Arc::new(AtomicBool::new(false)), no_events()).await
}

pub async fn run(b: &Bench, flow: Value) -> RunRecord {
    run_with(b, request(flow, json!({}))).await
}

pub fn flow(steps: Value) -> Value {
    json!({ "flow": "cluaiz.flow/1", "name": "test", "steps": steps })
}

pub fn steps_of(b: &Bench, run: &RunRecord) -> Vec<journal::StepRecord> {
    journal::read_journal(&b.paths.run_dir(&run.flow, &run.run_id)).finished
}
