//! UI-only reducer. The caller receives actions and remains the execution owner.

use crate::{
    approval::{Approval, ApprovalResponse},
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
    Approval(ApprovalResponse),
}

#[derive(Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub value: String,
    pub description: String,
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
    pub(crate) approval: Option<Approval>,
    pub(crate) approval_scroll: usize,
    pub(crate) approval_page: usize,
    pub(crate) approval_bottom: bool,
    saved_draft: Option<String>,
    pub(crate) suggestions: Vec<Suggestion>,
    pub(crate) selected: usize,
    pub(crate) help_open: bool,
    pub(crate) help_scroll: usize,
    pub(crate) help_page: usize,
    completion_start: usize,
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
            approval: None,
            approval_scroll: 0,
            approval_page: 1,
            approval_bottom: false,
            saved_draft: None,
            suggestions: Vec::new(),
            selected: 0,
            help_open: false,
            help_scroll: 0,
            help_page: 1,
            completion_start: 0,
        }
    }

    pub fn draft(&self) -> String {
        self.composer.text()
    }
    pub fn theme_picker_open(&self) -> bool {
        self.picker.is_some()
    }

    pub fn set_draft(&mut self, text: &str) -> Result<(), &'static str> {
        self.composer.replace(text)
    }

    pub fn open_themes(&mut self) {
        self.help_open = false;
        let selected = Theme::ALL
            .iter()
            .position(|theme| *theme == self.theme)
            .unwrap_or(0);
        self.picker = Some(ThemePicker {
            selected,
            original: self.theme,
        });
    }

    pub fn request_approval(&mut self, approval: Approval) -> Result<(), &'static str> {
        if self.approval.is_some() {
            return Err("An approval is already pending");
        }
        self.saved_draft = Some(self.composer.take());
        self.help_open = false;
        self.suggestions.clear();
        self.approval = Some(approval);
        self.approval_scroll = 0;
        self.approval_bottom = false;
        Ok(())
    }

    pub fn approval_pending(&self) -> bool {
        self.approval.is_some()
    }

    pub fn set_suggestions(&mut self, start: usize, suggestions: Vec<Suggestion>) {
        if !self.composer.cursor_at_end() {
            self.suggestions.clear();
            return;
        }
        self.completion_start = start;
        self.suggestions = suggestions;
        self.selected = 0;
    }

    pub fn clear_suggestions(&mut self) {
        self.suggestions.clear();
    }

    fn complete(&mut self) -> Option<String> {
        let item = self.suggestions.get(self.selected)?;
        let value = item.value.clone();
        let draft = self.draft();
        let prefix = draft.get(..self.completion_start)?;
        let updated = format!("{prefix}{value}");
        if let Err(error) = self.set_draft(&updated) {
            self.notice = Some(error.into());
            self.suggestions.clear();
            return None;
        }
        self.suggestions.clear();
        Some(value)
    }

    fn answer_approval(&mut self, answer: ApprovalResponse) -> Action {
        self.approval = None;
        let draft = self.saved_draft.take().unwrap_or_default();
        if let Err(error) = self.set_draft(&draft) {
            self.notice = Some(error.into());
        }
        Action::Approval(answer)
    }

    /// Release events never submit; repeat events may edit but cannot submit twice.
    pub fn handle(&mut self, event: Event) -> Action {
        let key = match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => key,
            Event::Paste(text) if self.picker.is_none() => {
                self.help_open = false;
                self.notice = self.composer.insert(&text).err().map(str::to_owned);
                return Action::None;
            }
            _ => return Action::None,
        };
        let intent = command(key);
        if !self.composer.cursor_at_end() {
            self.suggestions.clear();
        }
        if intent == Command::Exit {
            return Action::Exit;
        }
        if self.picker.is_some() {
            self.pick_theme(key.code, intent);
            return Action::None;
        }
        if self.help_open {
            match intent {
                Command::Help if key.kind == KeyEventKind::Press => self.help_open = false,
                Command::Escape => self.help_open = false,
                Command::PageUp => {
                    self.help_scroll = self.help_scroll.saturating_sub(self.help_page)
                }
                Command::PageDown => {
                    self.help_scroll = self.help_scroll.saturating_add(self.help_page)
                }
                Command::Themes => self.open_themes(),
                _ => {}
            }
            return Action::None;
        }
        if self.approval.is_some() {
            match intent {
                Command::Submit if key.kind == KeyEventKind::Press => {
                    let response = ApprovalResponse::parse(&self.composer.take());
                    if matches!(
                        response,
                        ApprovalResponse::Allow | ApprovalResponse::Session
                    ) && !self.approval_bottom
                    {
                        self.notice =
                            Some("Read the full operation with PgDn before allowing it".into());
                        return Action::None;
                    }
                    return self.answer_approval(response);
                }
                Command::Escape => return self.answer_approval(ApprovalResponse::Deny),
                Command::PageUp => {
                    self.approval_scroll = self.approval_scroll.saturating_sub(self.approval_page);
                    return Action::None;
                }
                Command::PageDown => {
                    self.approval_scroll = self.approval_scroll.saturating_add(self.approval_page);
                    return Action::None;
                }
                _ => {}
            }
        } else if !self.suggestions.is_empty() {
            match key.code {
                KeyCode::Up => {
                    self.selected = self.selected.saturating_sub(1);
                    return Action::None;
                }
                KeyCode::Down => {
                    self.selected = (self.selected + 1).min(self.suggestions.len() - 1);
                    return Action::None;
                }
                KeyCode::Tab => {
                    self.complete();
                    return Action::None;
                }
                KeyCode::Enter if key.kind == KeyEventKind::Press => {
                    if self.complete().is_some_and(|value| value.starts_with('/')) {
                        return Action::Submit(self.composer.take());
                    }
                    return Action::None;
                }
                KeyCode::Esc => {
                    self.suggestions.clear();
                    return Action::None;
                }
                _ => {}
            }
        }
        match intent {
            Command::Help
                if self.approval.is_none()
                    && self.draft().is_empty()
                    && key.kind == KeyEventKind::Press =>
            {
                self.suggestions.clear();
                self.help_scroll = 0;
                self.help_open = true;
            }
            Command::Help => {
                self.notice = self.composer.edit(key).err().map(str::to_owned);
            }
            Command::Thinking if key.kind == KeyEventKind::Press => {
                self.transcript.toggle_thinking()
            }
            Command::Themes => {
                self.open_themes();
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
