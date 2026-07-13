// BSL 1.1 — Cluaiz Technologies
//
// injectors/extension_injector.rs
//
// Dynamic C-Pointer rule compiler for Model Layer interaction with dynamic extensions (direct web scraping/parsing).

pub struct ExtensionRuleInjector;

impl ExtensionRuleInjector {
    pub fn compile_rules(ext_name: &str, cel_grammar: &str) -> String {
        format!(
            "[SYSTEM INSTRUCTION: You have access to an extension named '{}'. To execute this tool, output a JSON payload wrapped exactly inside `<TRIGGER:extension:{}>` and `</TRIGGER>`. Example: <TRIGGER:extension:{}>{}</TRIGGER>]",
            ext_name, ext_name, ext_name, cel_grammar
        )
    }
}
