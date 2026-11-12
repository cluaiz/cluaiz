//! Tools of layers outside the Engine (Automation's flows, `flow.<name>`) in a chat. They are
//! matched to the message like installed tools (`ToolSearch::suggest`), and only those that fit
//! are offered, so a chat without a fitting flow is as before. A call asks the owner on the chat's
//! approval card when the tool says so, then runs in the extension.

use crate::cep::extensions::{self, Caller, CONFIRM_META};
use crate::state::AppState;
use cluaiz_agents::tool_search::CatalogEntry;
use serde_json::{json, Map, Value};
use std::sync::Arc;

/// An extension's tool offered in this chat
#[derive(Debug, Clone)]
pub struct OfferedTool {
    /// The function name the model calls (letters, digits, `_`, `-`)
    pub function: String,
    /// The tool's own name (`flow.weekly-report`)
    pub tool: String,
    /// The owner is asked before it runs
    pub confirm: bool,
}

/// At most this many are offered in one turn
const MAX_OFFERED: usize = 5;

/// A tool name as a function name models accept
pub fn function_name(tool: &str) -> String {
    tool.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect()
}

/// The extensions' tools that fit the message: what to offer, their function schemas, and their
/// names for the hint
pub async fn offer(
    state: &Arc<AppState>,
    message: &str,
    previous: Option<&str>,
) -> (Vec<OfferedTool>, Vec<Value>) {
    let tools = extensions::tools(state).await;
    if tools.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let catalog: Vec<CatalogEntry> = tools
        .iter()
        .filter_map(|t| {
            let name = t["name"].as_str()?;
            let title = t["title"].as_str().unwrap_or_default();
            // The flow's own name, written as words: `weekly-report` is found by "weekly report"
            let words = name
                .split('.')
                .next_back()
                .unwrap_or(name)
                .replace(['-', '_'], " ");
            Some(CatalogEntry::named(
                name,
                "flow",
                t["description"].as_str().unwrap_or_default(),
                &[title, words.as_str()],
            ))
        })
        .collect();
    let picked: Vec<String> = cluaiz_agents::ToolSearch::suggest(&catalog, message, previous)
        .into_iter()
        .take(MAX_OFFERED)
        .map(|c| c.id)
        .collect();
    let mut offered = Vec::new();
    let mut schemas = Vec::new();
    for tool in tools.iter().filter(|t| {
        t["name"]
            .as_str()
            .is_some_and(|n| picked.iter().any(|p| p == n))
    }) {
        let name = tool["name"].as_str().unwrap_or_default().to_string();
        let function = function_name(&name);
        schemas.push(json!({ "type": "function", "function": {
            "name": function,
            "description": format!("{} (a saved flow, {name})", tool["description"].as_str().unwrap_or_default()),
            "parameters": tool["inputSchema"],
        }}));
        offered.push(OfferedTool {
            function,
            tool: name,
            confirm: tool["_meta"][CONFIRM_META].as_bool().unwrap_or(true),
        });
    }
    (offered, schemas)
}

/// The hint line naming the offered flows
pub fn hint(offered: &[OfferedTool]) -> String {
    let names: Vec<&str> = offered.iter().map(|o| o.function.as_str()).collect();
    cluaiz_core::prompts::render("chat.flow_hint", &[("flows", names.join(", ").as_str())])
}

/// Runs an offered tool for the chat, after the chat's own approval; the text the model reads
pub async fn run(
    state: &Arc<AppState>,
    tool: &OfferedTool,
    arguments: &str,
    chat_id: Option<&str>,
) -> Result<String, String> {
    let arguments: Map<String, Value> = match serde_json::from_str::<Value>(arguments) {
        Ok(Value::Object(map)) => map,
        Ok(Value::Null) | Err(_) if arguments.trim().is_empty() => Map::new(),
        _ => return Err("the arguments are not a JSON object".into()),
    };
    let caller = Caller {
        kind: "chat",
        chat_id: chat_id.map(String::from),
        confirmed: true,
    };
    let call = extensions::call_tool(state, &tool.tool, arguments, caller)
        .ok_or_else(|| format!("{} is no longer offered", tool.tool))?;
    match call.await {
        Ok(value) => Ok(value.to_string()),
        Err(error) => Err(match error.data {
            Some(data) => format!("{} {}", error.message, data),
            None => error.message,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::function_name;

    #[test]
    fn names_models_accept() {
        assert_eq!(function_name("flow.weekly-report"), "flow_weekly-report");
        assert_eq!(function_name("flow.a_b"), "flow_a_b");
    }
}
