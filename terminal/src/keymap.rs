//! One application shortcut map; text editing remains in the editor adapter.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Help,
    Thinking,
    Permissions,
    Clipboard,
    RemoveAttachment,
    ThinkingMode,
    Themes,
    Submit,
    Newline,
    Escape,
    Exit,
    PageUp,
    PageDown,
    Latest,
    Edit,
}

pub(crate) fn command(key: KeyEvent) -> Command {
    use Command::*;
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::ALT) => ThinkingMode,
        KeyCode::Char('v') if ctrl || key.modifiers.contains(KeyModifiers::ALT) => Clipboard,
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::ALT) => RemoveAttachment,
        KeyCode::BackTab => Permissions,
        KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => Permissions,
        KeyCode::Char('?') if !key.modifiers.contains(KeyModifiers::ALT) => Help,
        KeyCode::Char('/') if ctrl => Help,
        KeyCode::Char('o') if ctrl => Thinking,
        KeyCode::F(2) => Themes,
        KeyCode::Char('q') if ctrl => Exit,
        KeyCode::Char('c') if ctrl => Escape,
        KeyCode::Esc => Escape,
        KeyCode::Char('j') if ctrl => Newline,
        KeyCode::Enter
            if key
                .modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT) =>
        {
            Newline
        }
        KeyCode::Enter => Submit,
        KeyCode::PageUp => PageUp,
        KeyCode::PageDown => PageDown,
        KeyCode::End if ctrl => Latest,
        _ => Edit,
    }
}

/// Only controls implemented by this UI; execution commands come from the caller.
pub(crate) const SHORTCUTS: &[(&str, &str)] = &[
    ("?", "Shortcuts"),
    ("Enter", "Send"),
    ("Ctrl+J / Alt+Enter", "New line"),
    ("/", "Commands"),
    ("@", "Workspace files"),
    ("Tab / arrows", "Suggestions"),
    ("Ctrl+O", "Thinking / tool details"),
    ("Alt+T", "Thinking on / off"),
    ("Shift+Tab", "Ask / strict approvals"),
    ("Alt+V / Ctrl+V", "Paste image / file / text"),
    ("Alt+Backspace", "Remove last attachment"),
    ("F2", "Theme"),
    ("PgUp / PgDn", "Scroll"),
    ("Ctrl+End", "Latest output"),
    ("Ctrl+Z / Ctrl+Y", "Undo / redo"),
    ("Ctrl+Q", "Exit"),
];
