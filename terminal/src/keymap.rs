//! One application shortcut map; text editing remains in the editor adapter.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Help,
    Thinking,
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
        KeyCode::Char('?')
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            Help
        }
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
    ("? / Esc", "Open help on empty input / close help"),
    ("Enter", "Send your message"),
    (
        "Ctrl+J / Alt+Enter",
        "New line (Shift+Enter when supported)",
    ),
    ("/ / @", "Command / workspace filename suggestions"),
    ("Tab / arrows", "Choose a suggestion"),
    ("Ctrl+O", "Expand or collapse thinking"),
    ("F2", "Preview themes"),
    ("PgUp / PgDn", "Scroll conversation or open help"),
    ("Ctrl+End", "Return to latest output"),
    ("Ctrl+Z / Ctrl+Y", "Undo / redo draft edits"),
    ("Esc / Ctrl+C", "Interrupt; Esc denies a pending operation"),
    ("Ctrl+Q", "Exit and return to your shell"),
];
