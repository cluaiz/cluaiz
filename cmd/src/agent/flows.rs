//! Saved flows as tools of the terminal agent. Flows run on the server (Automation lives there),
//! so they are offered only while a server answers: the flows with a `tool` trigger, as its MCP
//! route lists them (`flow.<name>`). A call runs the flow with `flows.run` as the owner, after the
//! agent's own question when the flow asks for one (`confirm`), and waits for its outputs.

use crate::cli::server::{cep_reply, request_with_timeout};
use serde_json::{json, Value};
use std::time::Duration;

/// How long the agent waits for the server to list the flows
const LIST_TIMEOUT: Duration = Duration::from_secs(3);
/// One `tasks.result` wait
const RESULT_WAIT_MS: u64 = 5_000;
/// `_meta` key of a listed tool: a call asks the owner first
const CONFIRM_META: &str = "cluaiz/confirm";

#[derive(Debug, Clone)]
pub struct FlowTool {
    /// The function the model calls (`flow_weekly-report`)
    pub function: String,
    /// The flow's name
    pub name: String,
    pub confirm: bool,
    /// Its function schema
    pub schema: Value,
}

/// A tool name as a function name models accept
pub fn function_name(tool: &str) -> String {
    tool.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(64).collect()
}

/// MCP `tools/list` items of flows as the agent's tools
pub fn from_listed(tools: &[Value]) -> Vec<FlowTool> {
    tools
        .iter()
        .filter_map(|tool| {
            let listed = tool["name"].as_str()?;
            let name = listed.strip_prefix("flow.")?.to_string();
            let function = function_name(listed);
            let schema = json!({ "type": "function", "function": {
                "name": function,
                "description": format!("{} (a saved flow, {listed}; it runs on the Cluaiz server)", tool["description"].as_str().unwrap_or_default()),
                "parameters": tool["inputSchema"],
            }});
            Some(FlowTool { function, name, confirm: tool["_meta"][CONFIRM_META].as_bool().unwrap_or(true), schema })
        })
        .collect()
}

/// The flows the running server offers as tools; none when no server answers
pub async fn load() -> Vec<FlowTool> {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} });
    let Ok(response) = request_with_timeout(reqwest::Method::POST, "/mcp", LIST_TIMEOUT).json(&request).send().await else { return Vec::new() };
    let Ok(reply) = response.json::<Value>().await else { return Vec::new() };
    from_listed(reply["result"]["tools"].as_array().map(Vec::as_slice).unwrap_or_default())
}

fn message(error: &Value) -> String {
    error["message"].as_str().unwrap_or("failed").to_string()
}

/// Runs a flow and waits for it: its outputs as JSON, or why it did not end
pub async fn run(flow: &FlowTool, arguments: &Value) -> Result<String, String> {
    let params = json!({ "name": flow.name, "inputs": arguments, "trigger": { "kind": "tool", "caller": { "kind": "agent" } } });
    let started = cep_reply("flows.run", params).await.map_err(|e| format!("the Cluaiz server did not answer: {e}"))?.map_err(|e| format!("flows.run: {}", message(&e)))?;
    let run_id = started["run_id"].as_str().unwrap_or_default().to_string();
    let task_id = started["task"]["task_id"].as_str().ok_or("flows.run did not return a task")?.to_string();
    loop {
        match cep_reply("tasks.result", json!({ "task_id": task_id, "wait_ms": RESULT_WAIT_MS })).await.map_err(|e| e.to_string())? {
            Ok(run) => {
                return match run["status"].as_str() {
                    Some("succeeded") => Ok(json!({ "run_id": run_id, "status": "succeeded", "outputs": run["outputs"] }).to_string()),
                    status => Err(format!("the flow {} ended {}: {}", flow.name, status.unwrap_or("?"), run["error"]["message"].as_str().unwrap_or("no error message"))),
                };
            }
            Err(error) if error["data"]["type"] == "timeout" => {
                // A run that waits for a person goes on without the agent
                if let Ok(Ok(got)) = cep_reply("flows.run_get", json!({ "run_id": run_id, "steps": false })).await {
                    if got["run"]["status"] == "waiting" {
                        return Ok(json!({ "run_id": run_id, "status": "waiting", "waiting_for": got["run"]["waiting_for"], "note": "the run waits for a person; it goes on without you" }).to_string());
                    }
                }
            }
            Err(error) => return Err(format!("tasks.result: {}", message(&error))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_flows_become_functions() {
        let listed = vec![
            json!({ "name": "flow.weekly-report", "description": "A report", "inputSchema": { "type": "object" }, "_meta": { "cluaiz/confirm": false } }),
            json!({ "name": "fs.read", "description": "Not a flow" }),
        ];
        let flows = from_listed(&listed);
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].function, "flow_weekly-report");
        assert_eq!(flows[0].name, "weekly-report");
        assert!(!flows[0].confirm);
        assert_eq!(flows[0].schema["function"]["name"], "flow_weekly-report");
    }
}
