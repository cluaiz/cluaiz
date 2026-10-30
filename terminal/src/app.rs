//! UI-only reducer. The caller receives actions and remains the execution owner.

use crate::{
    input::Composer,
    keymap::{command, Command},
    model::{Limits, Metadata},
    theme::Theme,
    transcript::Transcript,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};

/// Actions leave the UI without running model, network or filesystem operations.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Submit(String),
    Cancel,
    Exit,
}

pub(crate) struct ThemePicker {
    pub selected: usize,
    original: Theme,
}

pub struct App {
    pub metadata: Metadata,
    pub theme: Theme,
    pub transcript: Transcript,
    pub notice: Option<String>,
    pub(crate) composer: Composer,
    pub(crate) picker: Option<ThemePicker>,
}

impl App {
    pub fn new(metadata: Metadata, limits: Limits) -> Self {
        Self {
            metadata,
            theme: Theme::Auto,
            transcript: Transcript::new(limits),
            notice: None,
            composer: Composer::new(limits),
            picker: None,
        }
    }

    pub fn draft(&self) -> String {
        self.composer.text()
    }
    pub fn theme_picker_open(&self) -> bool {
        self.picker.is_some()
    }

    /// Release events never submit; repeat events may edit but cannot submit twice.
    pub fn handle(&mut self, event: Event) -> Action {
        let key = match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => key,
            Event::Paste(text) if self.picker.is_none() => {
                self.notice = self.composer.insert(&text).err().map(str::to_owned);
                return Action::None;
            }
            _ => return Action::None,
        };
        let intent = command(key);
        if intent == Command::Exit {
            return Action::Exit;
        }
        if self.picker.is_some() {
            self.pick_theme(key.code, intent);
            return Action::None;
        }
        match intent {
            Command::Themes => {
                let selected = Theme::ALL
                    .iter()
                    .position(|theme| *theme == self.theme)
                    .unwrap_or(0);
                self.picker = Some(ThemePicker {
                    selected,
                    original: self.theme,
                });
            }
            Command::Submit if key.kind == KeyEventKind::Press => {
                if !self.draft().trim().is_empty() {
                    return Action::Submit(self.composer.take());
                }
            }
            Command::Newline => {
                self.notice = self.composer.insert("\n").err().map(str::to_owned);
            }
            Command::Escape => return Action::Cancel,
            Command::PageUp => self.transcript.scroll(-self.transcript.page_size()),
            Command::PageDown => self.transcript.scroll(self.transcript.page_size()),
            Command::Latest => self.transcript.latest(),
            Command::Edit => {
                self.notice = self.composer.edit(key).err().map(str::to_owned);
            }
            _ => {}
        }
        Action::None
    }

    fn pick_theme(&mut self, key: KeyCode, intent: Command) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        match (key, intent) {
            (_, Command::Escape) => {
                self.theme = picker.original;
                self.picker = None;
                return;
            }
            (_, Command::Submit) => {
                self.picker = None;
                return;
            }
            (KeyCode::Up, _) => picker.selected = picker.selected.saturating_sub(1),
            (KeyCode::Down, _) => picker.selected = (picker.selected + 1).min(Theme::ALL.len() - 1),
            _ => {}
        }
        self.theme = Theme::ALL[picker.selected];
    }
}
