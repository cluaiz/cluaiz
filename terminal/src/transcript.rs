//! Bounded display projection with cached wrapping and logical-line scroll anchors.

use crate::{model::Limits, sanitize::display_text};
use ratatui::{style::Stylize, text::Line};
use std::collections::VecDeque;

struct Entry {
    id: String,
    label: String,
    content: String,
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
        }
    }

    /// Updates a stable display entry, without manufacturing a tool result or writing a log.
    pub fn upsert(&mut self, id: &str, label: &str, content: &str) -> Result<(), &'static str> {
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
        };
        match self.entries.iter().position(|old| old.id == id) {
            Some(index) => self.entries[index] = entry,
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
        let width = usize::from(width.max(1));
        self.height = usize::from(height);
        if self.dirty || self.width != width {
            self.capture_anchor();
            self.reflow(width);
        }
        self.top = if self.follow {
            self.max_top()
        } else {
            self.top.min(self.max_top())
        };
        self.rows
            .iter()
            .skip(self.top)
            .take(self.height)
            .map(|row| row.text.clone())
            .collect()
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

    fn reflow(&mut self, width: usize) {
        self.rows.clear();
        for (index, entry) in self.entries.iter().enumerate() {
            self.rows.push(Row {
                entry: index,
                logical: 0,
                text: Line::from(entry.label.clone()).bold(),
            });
            for (logical, line) in entry.content.split('\n').enumerate() {
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
