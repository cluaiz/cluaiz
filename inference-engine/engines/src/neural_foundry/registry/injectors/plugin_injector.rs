// BSL 1.1 — Cluaiz Technologies
//
// injectors/plugin_injector.rs
//
// Dynamic C-Pointer rule compiler for Model Layer interaction with WASM plugins.

pub struct PluginRuleInjector;

impl PluginRuleInjector {
    pub fn compile_rules(plugin_name: &str, cel_grammar: &str) -> String {
        format!(
            "[SYSTEM INSTRUCTION: You have access to a WASM plugin named '{}'. To execute this tool, output a JSON payload wrapped exactly inside `<TRIGGER:plugin:{}>` and `</TRIGGER>`. Example: <TRIGGER:plugin:{}>{}</TRIGGER>]",
            plugin_name, plugin_name, plugin_name, cel_grammar
        )
    }
}
