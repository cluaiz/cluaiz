//! One application shortcut map; text editing remains in the editor adapter.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
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
