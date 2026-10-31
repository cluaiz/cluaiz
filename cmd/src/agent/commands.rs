//! The existing agent commands, shared by plain help and terminal completion.

use cluaiz_terminal::app::Suggestion;

pub const COMMANDS: &[(&str, &str)] = &[
    ("/model", "Change the model"),
    ("/mode", "Security mode: Full Access, Workspace-Gated, Strict Approval (as in the App), Plan"),
    ("/think", "Thinking: auto, on, off"),
    ("/effort", "Thinking effort: auto, low, medium, high, max"),
    ("/theme", "Colours of this screen"),
    ("/clear", "Start a new conversation"),
    ("/compact", "Replace the conversation with a summary to free context"),
    ("/undo", "Put back the files the last request changed"),
    ("/diff", "git diff of the files the last request changed"),
    ("/sessions", "Saved conversations of this folder (--continue / --resume <id>)"),
    ("!<command>", "Run a shell command yourself; the agent sees its output"),
    ("/help", "Commands and keyboard shortcuts"),
    ("/bye", "Close the agent"),
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
