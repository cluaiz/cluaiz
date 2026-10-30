//! Responsive rendering of a UI projection; only Screen writes terminal bytes.

use crate::{app::App, sanitize::display_text, theme::Theme};
use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

const MIN_WIDTH: u16 = 24;
const MIN_HEIGHT: u16 = 9;
const MAX_COMPOSER_LINES: usize = 6;

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let palette = app.theme.palette();
    frame.render_widget(Block::default().style(palette.base), area);
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        app.approval_bottom = false;
        frame.render_widget(
            Paragraph::new("Terminal too small. Resize, or Ctrl+Q to exit.").style(palette.base),
            area,
        );
        return;
    }
    let input_height = app.composer.line_count().clamp(1, MAX_COMPOSER_LINES) as u16 + 2;
    let [header, transcript, notice, composer, footer] = Layout::vertical([
        Constraint::Length(if area.height >= 14 { 3 } else { 2 }),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(input_height.min(area.height.saturating_sub(5))),
        Constraint::Length(1),
    ])
    .areas(area);
    let mut header_text = vec![Line::from(vec![Span::styled(
        display_text(&app.metadata.title),
        palette.accent.add_modifier(Modifier::BOLD),
    )])];
    if area.height >= 14 {
        header_text.push(Line::from(display_text(&app.metadata.workspace)).style(palette.muted));
    }
    header_text.push(Line::from(display_text(&app.metadata.status)).style(palette.muted));
    frame.render_widget(Paragraph::new(header_text), header);
    let rows = app.transcript.visible(transcript.width, transcript.height);
    frame.render_widget(Paragraph::new(rows).style(palette.base), transcript);
    if let Some(approval) = &app.approval {
        let width = transcript.width.saturating_sub(2).max(1) as usize;
        let detail = format!(
            "{}\n{}",
            display_text(&approval.title),
            display_text(&approval.detail)
        );
        let lines: Vec<_> = detail
            .split('\n')
            .flat_map(|line| {
                textwrap::wrap(line, width)
                    .into_iter()
                    .map(|part| Line::from(part.into_owned()))
            })
            .collect();
        let prompt_height = transcript
            .height
            .min((lines.len().saturating_add(2)).min(u16::MAX as usize) as u16);
        let prompt = Rect::new(
            transcript.x,
            transcript.bottom().saturating_sub(prompt_height),
            transcript.width,
            prompt_height,
        );
        let visible = prompt.height.saturating_sub(2) as usize;
        app.approval_page = visible.saturating_sub(1).max(1);
        let max_scroll = lines.len().saturating_sub(visible);
        app.approval_scroll = app.approval_scroll.min(max_scroll);
        app.approval_bottom = visible > 0 && app.approval_scroll == max_scroll;
        frame.render_widget(Clear, prompt);
        frame.render_widget(
            Paragraph::new(
                lines
                    .into_iter()
                    .skip(app.approval_scroll)
                    .take(visible)
                    .collect::<Vec<_>>(),
            )
            .block(Block::bordered().title(if app.approval_bottom {
                " Permission required "
            } else {
                " Permission · PgDn: read more "
            }))
            .style(palette.accent),
            prompt,
        );
    } else if !app.suggestions.is_empty() {
        let count = app.suggestions.len().min(5);
        let height = (count as u16 + 2).min(transcript.height);
        let menu = Rect::new(
            transcript.x,
            transcript.bottom().saturating_sub(height),
            transcript.width,
            height,
        );
        frame.render_widget(Clear, menu);
        let start = app.selected.saturating_sub(count.saturating_sub(1));
        let items = app
            .suggestions
            .iter()
            .enumerate()
            .skip(start)
            .take(count)
            .map(|(index, item)| {
                ListItem::new(format!(
                    "{} {}  {}",
                    if index == app.selected { ">" } else { " " },
                    display_text(&item.value),
                    display_text(&item.description)
                ))
                .style(if index == app.selected {
                    palette.accent.add_modifier(Modifier::BOLD)
                } else {
                    palette.base
                })
            });
        frame.render_widget(
            List::new(items)
                .block(Block::bordered().title(" Arrows / Tab / Enter "))
                .style(palette.base),
            menu,
        );
    }
    let message = app.notice.as_deref().map(display_text).unwrap_or_else(|| {
        if app.transcript.omitted_entries() > 0 {
            format!(
                "{} older display entries evicted; durable history belongs to the client",
                app.transcript.omitted_entries()
            )
        } else if !app.transcript.following() {
            "Reading earlier output | Ctrl+End: latest".into()
        } else {
            String::new()
        }
    });
    frame.render_widget(Paragraph::new(message).style(palette.muted), notice);
    app.composer.editor.set_style(palette.base);
    app.composer.editor.set_cursor_line_style(Style::default());
    app.composer
        .editor
        .set_cursor_style(palette.base.add_modifier(Modifier::REVERSED));
    app.composer.editor.set_placeholder_style(palette.muted);
    app.composer.editor.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(palette.muted),
    );
    frame.render_widget(&app.composer.editor, composer);
    let shortcuts = if app.approval.is_some() && area.width >= 65 {
        "y: once | a: tool/session | n/Esc: deny | instructions + Enter | PgUp/PgDn"
    } else if app.approval.is_some() {
        "y/a/n + Enter | PgDn | Esc"
    } else if area.width < 65 {
        "F2: theme | Ctrl+Q: exit"
    } else {
        "F2: theme | Alt+Enter/Ctrl+J: newline | PgUp/PgDn | Ctrl+Q: exit"
    };
    frame.render_widget(Paragraph::new(shortcuts).style(palette.muted), footer);
    if let Some(picker) = &app.picker {
        let popup = centered(area, 56, 8);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Block::bordered()
                .title(" Theme preview ")
                .style(palette.base),
            popup,
        );
        let inner = popup.inner(ratatui::layout::Margin::new(1, 1));
        let [choices, hint] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
        let items = Theme::ALL.iter().enumerate().map(|(index, theme)| {
            let selected = index == picker.selected;
            ListItem::new(format!(
                "{} {}",
                if selected { ">" } else { " " },
                theme.name()
            ))
            .style(if selected {
                palette.accent.add_modifier(Modifier::BOLD)
            } else {
                palette.base
            })
        });
        frame.render_widget(List::new(items), choices);
        let help = if inner.width < 48 {
            "Enter | Esc: cancel"
        } else {
            "Arrows: preview | Enter: apply | Esc: cancel"
        };
        frame.render_widget(Paragraph::new(help).style(palette.muted), hint);
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [column] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas::<1>(column)[0]
}
