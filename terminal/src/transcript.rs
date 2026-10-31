//! Bounded display projection with cached wrapping and logical-line scroll anchors.

use crate::{
    model::{Limits, Metadata},
    sanitize::display_text,
    theme::{Palette, Theme},
};
use ratatui::{style::Stylize, text::Line};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

struct Entry {
    id: String,
    label: String,
    content: String,
    role: Role,
    expanded: bool,
    intro: Option<Metadata>,
}

/// Semantic display kind supplied by the client, independent of labels and model output.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Role {
    Intro,
    User,
    Assistant,
    Thinking,
    ThinkingRunning,
    Tool,
    ToolRunning,
    Error,
    #[default]
    Notice,
}
struct Row {
    entry: usize,
    logical: usize,
    text: Line<'static>,
}

/// The client owns durable history. This object retains only a bounded display window.
pub struct Transcript {
    entries: VecDeque<Entry>,
    rows: Vec<Row>,
    width: usize,
    height: usize,
    top: usize,
    follow: bool,
    dirty: bool,
    anchor: Option<(String, usize)>,
    limits: Limits,
    omitted: usize,
    assistant_layout: Option<fn(&str, usize) -> Vec<String>>,
    active: Option<String>,
    palette: Option<Palette>,
    brand_blink: Option<(Instant, Vec<Line<'static>>)>,
}

impl Transcript {
    pub fn new(limits: Limits) -> Self {
        Self {
            entries: VecDeque::new(),
            rows: Vec::new(),
            width: 0,
            height: 0,
            top: 0,
            follow: true,
            dirty: false,
            anchor: None,
            limits,
            omitted: 0,
            assistant_layout: None,
            active: None,
            palette: None,
            brand_blink: None,
        }
    }

    /// Session metadata is an ordinary retained entry: it scrolls and can be evicted.
    pub(crate) fn intro(&mut self, metadata: &Metadata) {
        if self.limits.retained_entries > 0
            && !(metadata.title.is_empty()
                && metadata.brand.is_empty()
                && metadata.workspace.is_empty()
                && metadata.status.is_empty())
        {
            self.entries.push_back(Entry {
                id: "session-intro".into(),
                label: String::new(),
                content: String::new(),
                role: Role::Intro,
                expanded: true,
                intro: Some(metadata.clone()),
            });
            self.dirty = true;
        }
    }

    /// Activity is keyed by actual entry identity, never a label parsed from model text.
    pub fn set_active(&mut self, id: Option<&str>) {
        self.active = id.map(str::to_owned);
    }

    /// Remove transient preparation/writing rows; durable results use upsert instead.
    pub fn remove(&mut self, id: &str) {
        if self.entries.iter().any(|entry| entry.id == id) {
            self.capture_anchor();
            self.entries.retain(|entry| entry.id != id);
            self.dirty = true;
        }
    }

    /// Updates a stable display entry, without manufacturing a tool result or writing a log.
    pub fn upsert(&mut self, id: &str, label: &str, content: &str) -> Result<(), &'static str> {
        self.upsert_role(id, Role::Notice, label, content)
    }

    /// Keeps semantic roles out of label parsing. Collapsed thinking retains its full source.
    pub fn upsert_role(
        &mut self,
        id: &str,
        role: Role,
        label: &str,
        content: &str,
    ) -> Result<(), &'static str> {
        if content
            .len()
            .saturating_add(label.len())
            .saturating_add(id.len())
            > self.limits.entry_bytes
        {
            return Err("Display entry limit exceeded; request a bounded preview from the client");
        }
        if self.limits.retained_entries == 0 {
            return Err("Transcript retention is disabled");
        }
        self.capture_anchor();
        let entry = Entry {
            id: id.to_owned(),
            label: display_text(label),
            content: display_text(content),
            role,
            expanded: matches!(role, Role::ThinkingRunning | Role::ToolRunning),
            intro: None,
        };
        match self.entries.iter().position(|old| old.id == id) {
            Some(index) => {
                if self.entries[index].role == role
                    && self.entries[index].label == entry.label
                    && self.entries[index].content == entry.content
                {
                    return Ok(());
                }
                let completed = matches!(
                    (self.entries[index].role, role),
                    (Role::ToolRunning, Role::Tool) | (Role::ThinkingRunning, Role::Thinking)
                );
                let expanded = self.entries[index].expanded;
                self.entries[index] = entry;
                self.entries[index].expanded = !completed && expanded;
            }
            None => {
                self.entries.push_back(entry);
                if self.entries.len() > self.limits.retained_entries {
                    self.entries.pop_front();
                    self.omitted += 1;
                }
            }
        }
        self.dirty = true;
        Ok(())
    }

    /// Wraps only when content or width changes; clones only the visible rows for the frame.
    pub fn visible(&mut self, width: u16, height: u16) -> Vec<Line<'static>> {
        self.visible_styled(width, height, Theme::Auto.palette())
    }

    /// Applies the current theme to cached rows without rebuilding wrapping on theme changes.
    pub fn visible_styled(
        &mut self,
        width: u16,
        height: u16,
        palette: Palette,
    ) -> Vec<Line<'static>> {
        self.visible_animated(width, height, palette, None)
    }

    pub fn visible_animated(
        &mut self,
        width: u16,
        height: u16,
        palette: Palette,
        glyph: Option<&str>,
    ) -> Vec<Line<'static>> {
        let width = usize::from(width.max(1));
        let resized = self.height != usize::from(height);
        self.height = usize::from(height);
        if self.width != width || resized || self.palette != Some(palette) {
            self.brand_blink = None;
        }
        if self.dirty || self.width != width || resized || self.palette != Some(palette) {
            self.capture_anchor();
            self.reflow(width, palette);
            self.palette = Some(palette);
        }
        self.top = if self.follow {
            self.max_top()
        } else {
            self.top.min(self.max_top())
        };
        self.rows
            .iter()
            .enumerate()
            .skip(self.top)
            .take(self.height)
            .map(|(index, row)| {
                let role = self.entries[row.entry].role;
                let style = match role {
                    Role::Intro | Role::User => palette.base,
                    Role::Thinking | Role::ThinkingRunning => palette.muted.italic(),
                    Role::Tool | Role::ToolRunning | Role::Notice => palette.muted,
                    Role::Error => palette.accent.bold(),
                    Role::Assistant => palette.base,
                };
                let text = if role == Role::Intro {
                    self.brand_blink
                        .as_ref()
                        .and_then(|(_, lines)| lines.get(row.logical))
                        .unwrap_or(&row.text)
                } else {
                    &row.text
                };
                let mut line = text.clone().style(text.style.patch(style));
                if row.logical == 0
                    && (index == 0 || self.rows[index - 1].entry != row.entry)
                    && self.active.as_deref() == Some(self.entries[row.entry].id.as_str())
                {
                    if let Some(glyph) = glyph {
                        line.spans
                            .insert(0, ratatui::text::Span::raw(format!("{glyph} ")));
                    }
                }
                line
            })
            .collect()
    }

    pub fn toggle_thinking(&mut self) {
        self.capture_anchor();
        let expand = self
            .entries
            .iter()
            .any(|entry| matches!(entry.role, Role::Thinking | Role::Tool) && !entry.expanded);
        for entry in &mut self.entries {
            if matches!(entry.role, Role::Thinking | Role::Tool) {
                entry.expanded = expand;
            }
        }
        self.dirty = true;
    }

    /// The client may reuse its Markdown layout without introducing a second parser here.
    pub fn set_assistant_layout(&mut self, layout: fn(&str, usize) -> Vec<String>) {
        self.assistant_layout = Some(layout);
        self.dirty = true;
    }

    /// Mouse hit-testing uses rendered row identity; model text cannot become an action.
    pub fn click(&mut self, visible_row: usize, column: usize) {
        let Some(row) = self.rows.get(self.top.saturating_add(visible_row)) else {
            return;
        };
        if let Some(metadata) = &self.entries[row.entry].intro {
            // Only a row of the art itself (drawn above the details) is a click target
            let text: String = row.text.spans.iter().map(|span| span.content.as_ref()).collect();
            let art_row = !text.trim().is_empty()
                && metadata.brand.lines().any(|line| line.trim_end() == text.trim_end());
            if metadata.brand_blink.is_empty()
                || !art_row
                || column >= row.text.width()
            {
                return;
            }
            let Some(palette) = self.palette else { return };
            let area = ratatui::layout::Rect::new(0, 0, self.width as u16, self.height as u16);
            let original = crate::render::header(metadata, area, palette, None);
            let blink = crate::render::header(metadata, area, palette, Some(metadata.brand_blink));
            // Never move details or change scroll geometry for an incompatible frame.
            let shape = |line: &Line<'_>| {
                (
                    line.width(),
                    line.spans.len(),
                    line.spans.first().map(|span| span.width()),
                )
            };
            if original.iter().map(shape).eq(blink.iter().map(shape)) {
                self.brand_blink = Some((Instant::now(), blink));
            }
            return;
        }
        self.toggle_row(visible_row);
    }

    /// Reuses the caller's existing UI tick, including one final redraw on expiry.
    pub fn advance_animation(&mut self) -> bool {
        let Some((started, _)) = &self.brand_blink else {
            return false;
        };
        if started.elapsed() >= Duration::from_millis(180) {
            self.brand_blink = None;
        }
        true
    }

    /// Toggle a completed tool/thinking entry without affecting running work.
    pub fn toggle_row(&mut self, visible_row: usize) {
        let Some(row) = self.rows.get(self.top.saturating_add(visible_row)) else {
            return;
        };
        if row.logical != 0 {
            return;
        }
        let index = row.entry;
        if matches!(self.entries[index].role, Role::Thinking | Role::Tool) {
            self.capture_anchor();
            self.entries[index].expanded = !self.entries[index].expanded;
            self.dirty = true;
        }
    }

    /// Moves within rendered rows. Positive values move toward newer output.
    pub fn scroll(&mut self, rows: isize) {
        self.top = self.top.saturating_add_signed(rows).min(self.max_top());
        self.follow = self.top == self.max_top();
        self.anchor = None;
    }

    pub fn latest(&mut self) {
        self.follow = true;
        self.anchor = None;
    }
    pub fn following(&self) -> bool {
        self.follow
    }
    pub fn omitted_entries(&self) -> usize {
        self.omitted
    }
    pub fn page_size(&self) -> isize {
        self.height.saturating_sub(1).max(1) as isize
    }

    fn max_top(&self) -> usize {
        self.rows.len().saturating_sub(self.height)
    }

    fn capture_anchor(&mut self) {
        if self.follow || self.anchor.is_some() || self.dirty {
            return;
        }
        if let Some(row) = self.rows.get(self.top) {
            if let Some(entry) = self.entries.get(row.entry) {
                self.anchor = Some((entry.id.clone(), row.logical));
            }
        }
    }

    fn reflow(&mut self, width: usize, palette: Palette) {
        self.rows.clear();
        for (index, entry) in self.entries.iter().enumerate() {
            if let Some(metadata) = &entry.intro {
                for (logical, text) in crate::render::header(
                    metadata,
                    ratatui::layout::Rect::new(0, 0, width as u16, self.height as u16),
                    palette,
                    None,
                )
                .into_iter()
                .enumerate()
                {
                    for text in if text.width() > width {
                        textwrap::wrap(&text.to_string(), width)
                            .into_iter()
                            .map(|part| Line::from(part.into_owned()).style(text.style))
                            .collect()
                    } else {
                        vec![text]
                    } {
                        self.rows.push(Row {
                            entry: index,
                            logical,
                            text,
                        });
                    }
                }
                self.rows.push(Row {
                    entry: index,
                    logical: usize::MAX,
                    text: Line::default(),
                });
                continue;
            }
            let prefix = match entry.role {
                Role::Intro => "",
                Role::User => "> ",
                Role::Assistant => "● ",
                Role::Thinking | Role::Tool if entry.expanded => "▾ ",
                Role::Thinking | Role::Tool => "▸ ",
                Role::ThinkingRunning | Role::ToolRunning => "▾ ",
                Role::Error => "! ",
                Role::Notice => "",
            };
            let collapsed = matches!(entry.role, Role::Thinking | Role::Tool) && !entry.expanded;
            let label = format!(
                "{prefix}{}{}",
                entry.label,
                if collapsed { " · Ctrl+O / click" } else { "" }
            );
            for part in textwrap::wrap(&label, width.saturating_sub(2).max(1))
                .into_iter()
                .filter(|_| entry.role != Role::Assistant)
            {
                self.rows.push(Row {
                    entry: index,
                    logical: 0,
                    text: Line::from(part.into_owned()).bold(),
                });
            }
            let formatted = if entry.role == Role::Assistant {
                self.assistant_layout
                    .map(|layout| layout(&entry.content, width))
            } else {
                None
            };
            let lines: Vec<_> = formatted
                .as_ref()
                .map(|rows| rows.iter().map(String::as_str).collect())
                .unwrap_or_else(|| entry.content.split('\n').collect());
            for (logical, line) in lines.into_iter().enumerate().filter(|_| !collapsed) {
                for part in textwrap::wrap(line, width) {
                    self.rows.push(Row {
                        entry: index,
                        logical: logical + 1,
                        text: Line::from(part.into_owned()),
                    });
                }
            }
            self.rows.push(Row {
                entry: index,
                logical: usize::MAX,
                text: Line::default(),
            });
        }
        if let Some((id, logical)) = self.anchor.take() {
            self.top = self
                .rows
                .iter()
                .position(|row| {
                    row.logical == logical
                        && self
                            .entries
                            .get(row.entry)
                            .is_some_and(|entry| entry.id == id)
                })
                .unwrap_or(0);
        }
        self.width = width;
        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blink_tick_restores_once_and_then_goes_idle() {
        let mut transcript = Transcript::new(Limits::default());
        transcript.brand_blink = Some((Instant::now(), vec![Line::from("blink")]));
        assert!(transcript.advance_animation());
        assert!(transcript.brand_blink.is_some());
        transcript.brand_blink.as_mut().unwrap().0 = Instant::now() - Duration::from_secs(1);
        assert!(transcript.advance_animation());
        assert!(transcript.brand_blink.is_none());
        assert!(!transcript.advance_animation());
    }
}
