//! The existing agent commands, shared by plain help and terminal completion.

use cluaiz_terminal::app::Suggestion;

pub const COMMANDS: &[(&str, &str)] = &[
    ("/help", "Show commands and keyboard controls"),
    ("/theme", "Choose the terminal theme"),
    ("/yes", "Allow tool changes for this session"),
    ("/ask", "Ask before tool changes again"),
    ("/strict", "Require approval for every built-in tool"),
    (
        "/permissions",
        "Explain current approval and sandbox boundaries",
    ),
    ("/think", "Toggle model thinking; accepts on / off / auto"),
    ("/clear", "Clear this agent's conversation"),
    ("/bye", "Close the agent and return to the shell"),
];

pub fn help() -> String {
    COMMANDS
        .iter()
        .map(|(name, description)| format!("{name:<12} {description}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn suggestions(prefix: &str) -> Vec<Suggestion> {
    COMMANDS
        .iter()
        .filter(|(name, _)| name.starts_with(prefix))
        .map(|(name, description)| Suggestion {
            value: (*name).into(),
            description: (*description).into(),
        })
        .collect()
}
