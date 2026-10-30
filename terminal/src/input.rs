//! Bounded adapter around the maintained multiline editor; no shell or command dispatch here.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::{CursorMove, TextArea, WrapMode};

use crate::{model::Limits, sanitize::display_text};

pub struct Composer {
    pub(crate) editor: TextArea<'static>,
    limits: Limits,
}

impl Composer {
    pub fn new(limits: Limits) -> Self {
        let mut editor = TextArea::default();
        editor.set_wrap_mode(WrapMode::WordOrGlyph);
        editor.set_max_histories(limits.edit_history);
        editor.set_placeholder_text("Type a message");
        Self { editor, limits }
    }

    /// Retains the whole draft; wrapping never changes its contents.
    pub fn text(&self) -> String {
        self.editor.lines().join("\n")
    }

    pub fn line_count(&self) -> usize {
        self.editor.lines().len()
    }

    /// Paste is insertion only. Embedded newlines and command prefixes cannot submit.
    pub fn insert(&mut self, text: &str) -> Result<(), &'static str> {
        if text.len().saturating_add(self.byte_len()) > self.limits.input_bytes {
            return Err("Input limit reached; the draft was kept unchanged");
        }
        self.editor.insert_str(display_text(text));
        Ok(())
    }

    /// Editing uses widget operations; application shortcuts are handled before reaching this method.
    pub fn edit(&mut self, key: KeyEvent) -> Result<(), &'static str> {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Left => self.editor.move_cursor(CursorMove::Back),
            KeyCode::Right => self.editor.move_cursor(CursorMove::Forward),
            KeyCode::Up => self.editor.move_cursor(CursorMove::Up),
            KeyCode::Down => self.editor.move_cursor(CursorMove::Down),
            KeyCode::Home => self.editor.move_cursor(CursorMove::Head),
            KeyCode::End => self.editor.move_cursor(CursorMove::End),
            KeyCode::Char('z') if control => {
                self.editor.undo();
            }
            KeyCode::Char('y') if control => {
                self.editor.redo();
            }
            KeyCode::Char(_)
                if key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {}
            KeyCode::Char(c) => return self.insert(&c.to_string()),
            KeyCode::Enter => return self.insert("\n"),
            KeyCode::Tab => return self.insert("    "),
            KeyCode::Backspace | KeyCode::Delete => {
                self.editor.input_without_shortcuts(key);
            }
            _ => {}
        }
        Ok(())
    }

    pub fn take(&mut self) -> String {
        let text = self.text();
        *self = Self::new(self.limits);
        text
    }

    fn byte_len(&self) -> usize {
        self.editor.lines().iter().map(String::len).sum::<usize>()
            + self.editor.lines().len().saturating_sub(1)
    }
}
