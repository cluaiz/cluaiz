//! What is installed and callable on this Engine, read through the Engine's own methods
//! (`tools.list`, `models.list`) and its method registry: for checks, dry runs, and the palette
//! of `flows.nodes`.

use cluaiz_api::cep::CallCtx;
use cluaiz_api::AppState;
use cluaiz_automation::host::{Catalog, NodeInfo};
use cluaiz_automation::permissions;
use cluaiz_cep::envelope::Meta as CallMeta;
use cluaiz_cep::{Execution, Principal, Scope};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Default)]
pub struct Snapshot {
    tools: BTreeMap<String, NodeInfo>,
    methods: BTreeMap<String, NodeInfo>,
    /// Installed chat models; `None` when the list could not be read
    models: Option<Vec<String>>,
    /// What a step can run here, as `flows.nodes` lists it
    nodes: Vec<Value>,
}

/// A tool function as a node of the palette
fn tool_node(reference: String, source: &str, function: &Value, entry: &Value) -> Value {
    let info = function_info(function);
    let hint = |name: &str| function.get("annotations").and_then(|a| a.get(name)).and_then(Value::as_bool).unwrap_or(false);
    let title = function.get("title").or_else(|| function.get("annotations").and_then(|a| a.get("title"))).or_else(|| function.get("name")).cloned().unwrap_or(Value::Null);
    json!({
        "ref": reference,
        "step": "tool",
        "source": source,
        "title": title,
        "description": info.description,
        "input_schema": info.input_schema,
        "output_schema": function.get("outputSchema").or_else(|| function.get("output_schema")).cloned(),
        "sends": info.sends || info.network,
        "destructive": hint("destructiveHint"),
        "needs_approval": entry.get("security_mode").and_then(Value::as_str) == Some("require_approval"),
    })
}

fn function_info(function: &Value) -> NodeInfo {
    let hint = |name: &str| function.get("annotations").and_then(|a| a.get(name)).and_then(Value::as_bool);
    let schema = function.get("inputSchema").or_else(|| function.get("input_schema")).or_else(|| function.get("parameters")).cloned();
    let sends = schema.as_ref().and_then(|s| s.get("x-cluaiz-sends")).and_then(Value::as_bool).unwrap_or(false);
    let description = function.get("description").and_then(Value::as_str).map(String::from);
    NodeInfo { description, input_schema: schema, scope: Some("exec".into()), read_only: hint("readOnlyHint") == Some(true), network: hint("openWorldHint") == Some(true), sends, task: false }
}

impl Snapshot {
    /// Reads the catalog as the owner (only reading methods are called)
    pub async fn read(state: &Arc<AppState>) -> Snapshot {
        let mut snapshot = Snapshot::default();
        for method in state.cep.registry().methods() {
            // A flow may call what is neither refused to flows nor the owner's alone
            if !permissions::not_callable(method.name) && !method.meta.owner_only && method.meta.scope != Scope::Admin {
                snapshot.nodes.push(json!({
                    "ref": method.name,
                    "step": "call",
                    "source": "method",
                    "title": method.name,
                    "description": method.meta.summary,
                    "input_schema": method.params_schema,
                    "output_schema": method.result_schema,
                    "scope": method.meta.scope.name(),
                    "sends": false,
                    "destructive": method.meta.destructive,
                    "needs_approval": false,
                }));
            }
            snapshot.methods.insert(
                method.name.to_string(),
                NodeInfo { description: Some(method.meta.summary.to_string()), input_schema: Some(method.params_schema.clone()), scope: Some(method.meta.scope.name().to_string()), read_only: method.meta.scope == Scope::Read, network: false, sends: false, task: method.meta.execution == Execution::Task },
            );
        }
        let owner = Arc::new(Principal::owner());
        let call = |method: &'static str, params: Map<String, Value>| {
            let ctx = CallCtx { state: state.clone() };
            let owner = owner.clone();
            async move { state.cep.invoke(ctx, owner, method, params, CallMeta::default()).await }
        };
        if let Ok(Value::Array(tools)) = call("tools.list", Map::new()).await {
            for tool in tools {
                let (Some(kind), Some(id)) = (tool.get("category").and_then(Value::as_str), tool.get("id").and_then(Value::as_str)) else { continue };
                let functions = tool.get("functions").and_then(Value::as_array).cloned().unwrap_or_default();
                let enabled = tool.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                match kind {
                    "skill" => {
                        snapshot.tools.insert(format!("skill:{id}"), NodeInfo { read_only: true, ..Default::default() });
                        if enabled {
                            let description = tool.get("description").cloned().unwrap_or(Value::Null);
                            snapshot.nodes.push(json!({ "ref": format!("skill:{id}"), "step": "tool", "source": "skill", "title": tool.get("name").cloned().unwrap_or(json!(id)), "description": description, "input_schema": null, "output_schema": null, "sends": false, "destructive": false, "needs_approval": false }));
                        }
                    }
                    "mcp" | "plugin" => {
                        for function in &functions {
                            if let Some(name) = function.get("name").and_then(Value::as_str) {
                                snapshot.tools.insert(format!("{kind}:{id}/{name}"), function_info(function));
                                if enabled {
                                    snapshot.nodes.push(tool_node(format!("{kind}:{id}/{name}"), kind, function, &tool));
                                }
                            }
                        }
                        if kind == "plugin" {
                            snapshot.tools.insert(format!("plugin:{id}"), functions.first().map(function_info).unwrap_or_default());
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut models = Map::new();
        models.insert("category".into(), Value::String("chat".into()));
        if let Ok(Value::Array(list)) = call("models.list", models).await {
            snapshot.models = Some(list.iter().filter_map(|m| m.get("id").and_then(Value::as_str)).map(String::from).collect());
        }
        snapshot
    }
}

impl Snapshot {
    /// The palette: installed tools first, then methods
    pub fn nodes(&self) -> Vec<Value> {
        let mut nodes = self.nodes.clone();
        nodes.sort_by_key(|n| (n["step"] != "tool", n["ref"].as_str().unwrap_or_default().to_string()));
        nodes
    }
}

impl Catalog for Snapshot {
    fn tool(&self, name: &str) -> Option<NodeInfo> {
        self.tools.get(name).cloned()
    }
    fn method(&self, name: &str) -> Option<NodeInfo> {
        self.methods.get(name).cloned()
    }
    fn names(&self) -> Vec<String> {
        self.tools.keys().chain(self.methods.keys()).cloned().collect()
    }
    fn model_installed(&self, model: &str) -> Option<bool> {
        // A provider's model (`<provider>/<model>`) cannot be checked without asking the provider
        if model.contains('/') {
            return None;
        }
        self.models.as_ref().map(|list| list.iter().any(|m| m == model))
    }
}
