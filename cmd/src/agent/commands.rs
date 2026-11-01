//! The existing agent commands, shared by plain help and terminal completion.

use cluaiz_terminal::app::Suggestion;

pub const COMMANDS: &[(&str, &str)] = &[
    ("/model", "Change the model"),
    ("/mode", "Security mode: Full Access, Workspace-Gated, Strict Approval (as in the App), Plan"),
    ("/think", "Thinking: auto, on, off"),
    ("/effort", "Thinking effort: auto, low, medium, high, max"),
    ("/theme", "Colours of this screen"),
    ("/clear", "Start a new conversation"),
    ("/plan", "Where the model runs: GPU / RAM blocks, context, KV cache, memory, and why"),
    ("/compact", "Replace the conversation with a summary to free context"),
    ("/undo", "Put back the files the last request changed"),
    ("/rewind", "Checkpoints of this conversation; /rewind <n> puts files and conversation back to before request n"),
    ("/diff", "git diff of the files the last request changed"),
    ("/code", "Open the folder in VS Code with the changed files (/code <path>: that file)"),
    ("/init", "The model reads this project and writes its AGENTS.md"),
    ("/memory","The AGENTS.md instructions in use (~/.cluaiz/AGENTS.md, then the repository's)"),
    ("/skills", "The SKILL.md skills the model can use (.agents/skills, ~/.agents/skills)"),
    ("/rules", "Allow and deny rules in use, from ~/.cluaiz/config/agent.json, .cluaiz/agent.json, .cluaiz/agent.local.json"),
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
