// BSL 1.1 — Cluaiz Technologies
//
// injectors/mcp_injector.rs
//
// Dynamic C-Pointer rule compiler for Model Layer interaction with MCP gateways.

pub struct McpRuleInjector;

impl McpRuleInjector {
    pub fn compile_rules(mcp_name: &str) -> String {
        format!(
            "[SYSTEM INSTRUCTION: You have access to MCP server '{}'. To execute this tool, output a JSON payload wrapped exactly inside `<TRIGGER:mcp:{}>` and `</TRIGGER>`. Example: <TRIGGER:mcp:{}>{{\"command\": \"...\"}}</TRIGGER>]",
            mcp_name, mcp_name, mcp_name
        )
    }
}
